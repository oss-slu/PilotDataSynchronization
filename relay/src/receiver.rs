//! A stand-in for iMotions inside the window.
//!
//! `src/server/` already does this, but it is a separate binary: a terminal, a
//! `cargo run`, and a port to keep straight. Someone checking the relay works
//! before a session should need none of that, and in the lab iMotions runs on
//! another machine, so there is no way to try the relay at a desk without
//! something local to send to.
//!
//! It binds port 0 and reports the port it was given, so it cannot collide
//! with the real receiver, with the mock server, or with a copy of itself left
//! running. The window reads the port back and fills the address in, which is
//! the part that would otherwise be typed.

use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossbeam::channel::{unbounded, Receiver, Sender};

/// How often the thread reports what it has seen. Fast enough to look live,
/// slow enough that a 160 packet a second stream does not flood the channel
/// with one message per packet.
const REPORT_EVERY: Duration = Duration::from_millis(250);

/// How long a read waits before the thread checks whether it has been stopped.
const READ_TIMEOUT: Duration = Duration::from_millis(100);

/// What the receiver has seen so far.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Tally {
    pub(crate) packets: usize,
    pub(crate) bytes: usize,
    pub(crate) connected: bool,
}

enum Report {
    Tally(Tally, Option<String>),
}

/// A local receiver the relay can be pointed at.
pub(crate) struct TestReceiver {
    pub(crate) port: u16,
    pub(crate) tally: Tally,
    pub(crate) last_packet: Option<String>,
    stop: Arc<AtomicBool>,
    reports: Receiver<Report>,
    handle: Option<JoinHandle<()>>,
}

impl TestReceiver {
    /// Bind a port and start listening. Fails only if no port can be had.
    pub(crate) fn start() -> Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .context("Could not open a local port for the test receiver")?;
        let port = listener
            .local_addr()
            .context("The test receiver's port could not be read back")?
            .port();
        listener
            .set_nonblocking(true)
            .context("The test receiver's socket could not be made non-blocking")?;

        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = unbounded();
        let handle = {
            let stop = Arc::clone(&stop);
            thread::spawn(move || serve(listener, stop, tx))
        };

        Ok(Self {
            port,
            tally: Tally::default(),
            last_packet: None,
            stop,
            reports: rx,
            handle: Some(handle),
        })
    }

    /// The address to connect the relay to.
    pub(crate) fn addr(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    /// Take in whatever the thread has reported. Called from the window's tick.
    pub(crate) fn poll(&mut self) {
        for report in self.reports.try_iter() {
            match report {
                Report::Tally(tally, last) => {
                    self.tally = tally;
                    if last.is_some() {
                        self.last_packet = last;
                    }
                }
            }
        }
    }

    /// One line for the window.
    pub(crate) fn describe(&self) -> String {
        if !self.tally.connected && self.tally.packets == 0 {
            return format!(
                "Test receiver listening on {}. Press Connect iMotions to send to it.",
                self.addr()
            );
        }
        format!(
            "Test receiver on {}: {} packets, {} bytes",
            self.addr(),
            self.tally.packets,
            self.tally.bytes
        )
    }
}

impl Drop for TestReceiver {
    /// The window owns this, so closing the window has to end the thread.
    /// Without it the port would stay bound until the process exited, which is
    /// the same thing the mock server is awkward about.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Accept one connection at a time and count what arrives.
///
/// One at a time on purpose: iMotions is one destination, and accepting a
/// second would hide the relay opening a connection it never closed.
fn serve(listener: TcpListener, stop: Arc<AtomicBool>, tx: Sender<Report>) {
    let mut tally = Tally::default();

    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                tally.connected = true;
                let _ = tx.send(Report::Tally(tally, None));
                read_until_closed(stream, &stop, &tx, &mut tally);
                tally.connected = false;
                let _ = tx.send(Report::Tally(tally, None));
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return,
        }
    }
}

/// Count packets on one connection until it closes or the receiver is stopped.
fn read_until_closed(
    stream: TcpStream,
    stop: &Arc<AtomicBool>,
    tx: &Sender<Report>,
    tally: &mut Tally,
) {
    if stream.set_read_timeout(Some(READ_TIMEOUT)).is_err() {
        return;
    }
    let mut stream = stream;
    let mut buffer = [0u8; 8192];
    let mut pending = String::new();
    let mut last_reported = Instant::now();
    let mut last_packet: Option<String> = None;

    loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                tally.bytes += read;
                pending.push_str(&String::from_utf8_lossy(&buffer[..read]));
                // The relay ends every packet with CRLF, so a line is a packet.
                while let Some(end) = pending.find('\n') {
                    let line: String = pending.drain(..=end).collect();
                    let line = line.trim_end_matches(['\r', '\n']).to_string();
                    if !line.is_empty() {
                        tally.packets += 1;
                        last_packet = Some(line);
                    }
                }
                // A partial packet left in `pending` is not a leak: the next
                // read completes it. Only a peer that sends an endless line
                // without a newline could grow it, and the relay never does.
            }
            Err(ref e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => break,
        }

        if last_reported.elapsed() >= REPORT_EVERY {
            let _ = tx.send(Report::Tally(*tally, last_packet.take()));
            last_reported = Instant::now();
        }
    }
    let _ = tx.send(Report::Tally(*tally, last_packet));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Wait for a condition the receiver's thread has to report, so the test
    /// does not depend on a sleep being long enough.
    fn wait_for(receiver: &mut TestReceiver, what: &str, mut done: impl FnMut(&TestReceiver) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            receiver.poll();
            if done(receiver) {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out waiting for {what}: {:?}", receiver.tally);
    }

    #[test]
    fn it_binds_a_port_of_its_own_and_says_which() {
        let receiver = TestReceiver::start().unwrap();
        assert_ne!(receiver.port, 0, "port 0 means nothing was bound");
        assert_eq!(receiver.addr(), format!("127.0.0.1:{}", receiver.port));
    }

    #[test]
    fn two_receivers_do_not_collide() {
        let first = TestReceiver::start().unwrap();
        let second = TestReceiver::start().unwrap();
        assert_ne!(first.port, second.port);
    }

    #[test]
    fn it_counts_the_packets_it_is_sent() {
        let mut receiver = TestReceiver::start().unwrap();
        let mut client = TcpStream::connect(receiver.addr()).unwrap();
        client
            .write_all(b"E;1;PilotDataSync;;;;;AltitudeSync;3000;3000\r\nE;1;PilotDataSync;;;;;HeadingSync;270;270\r\n")
            .unwrap();
        client.flush().unwrap();

        wait_for(&mut receiver, "two packets", |r| r.tally.packets >= 2);
        assert_eq!(receiver.tally.packets, 2);
        assert!(receiver.tally.bytes > 0);
        assert!(
            receiver.last_packet.as_deref().unwrap().contains("HeadingSync"),
            "{:?}",
            receiver.last_packet
        );
    }

    #[test]
    fn a_packet_split_across_two_writes_is_counted_once() {
        let mut receiver = TestReceiver::start().unwrap();
        let mut client = TcpStream::connect(receiver.addr()).unwrap();
        client.write_all(b"E;1;PilotDataSync;;;;;Altitude").unwrap();
        client.flush().unwrap();
        thread::sleep(Duration::from_millis(150));
        client.write_all(b"Sync;3000;3000\r\n").unwrap();
        client.flush().unwrap();

        wait_for(&mut receiver, "one packet", |r| r.tally.packets >= 1);
        assert_eq!(receiver.tally.packets, 1);
        assert!(receiver.last_packet.as_deref().unwrap().contains("AltitudeSync"));
    }

    #[test]
    fn it_notices_the_connection_and_then_its_loss() {
        let mut receiver = TestReceiver::start().unwrap();
        let client = TcpStream::connect(receiver.addr()).unwrap();
        wait_for(&mut receiver, "the connection", |r| r.tally.connected);
        drop(client);
        wait_for(&mut receiver, "the disconnection", |r| !r.tally.connected);
    }

    #[test]
    fn a_second_flight_can_connect_after_the_first_closes() {
        let mut receiver = TestReceiver::start().unwrap();
        let mut first = TcpStream::connect(receiver.addr()).unwrap();
        first.write_all(b"one\r\n").unwrap();
        first.flush().unwrap();
        wait_for(&mut receiver, "the first packet", |r| r.tally.packets >= 1);
        drop(first);

        let mut second = TcpStream::connect(receiver.addr()).unwrap();
        second.write_all(b"two\r\n").unwrap();
        second.flush().unwrap();
        wait_for(&mut receiver, "the second packet", |r| r.tally.packets >= 2);
        assert_eq!(receiver.tally.packets, 2, "the count carries across connections");
    }

    #[test]
    fn dropping_it_frees_the_port() {
        let receiver = TestReceiver::start().unwrap();
        let port = receiver.port;
        drop(receiver);
        // The thread has joined by now, so the port is ours to take.
        let reused = TcpListener::bind(("127.0.0.1", port));
        assert!(reused.is_ok(), "the port was still held: {:?}", reused.err());
    }

    #[test]
    fn it_says_what_to_do_before_anything_has_connected() {
        let receiver = TestReceiver::start().unwrap();
        let line = receiver.describe();
        assert!(line.contains("Connect iMotions"), "{line}");
        assert!(line.contains(&receiver.port.to_string()), "{line}");
    }
}
