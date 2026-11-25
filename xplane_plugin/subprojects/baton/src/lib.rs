/*
This file is the heart of this Rust submodule. The crate in use, cxx, is used to create a safe interop
layer between Rust and C++. See the high level overview in the top-level project root README.md for details
on why we want this.

Keep in mind that what is shown here is not entirely typical of Rust code, as this crate takes liberties
in order to facilitate the aforementioned interoperability.
*/

use crossbeam::channel::{unbounded, Receiver, Sender};
use cxx::CxxVector;
use interprocess::local_socket::{
    prelude::*, GenericFilePath, GenericNamespaced, NameType, Stream, ToFsName, ToNsName,
};
use std::{
    io::{prelude::*, BufReader},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant, SystemTime},
};

/// The log lives in the temp folder (%TEMP% on Windows), where X-Plane can write it.
const LOG_NAME: &str = "baton_debug.log";
/// Where the previous run's log is kept when baton starts again.
const OLD_LOG_NAME: &str = "baton_debug.old.log";
/// How often the send loop writes a "sent N samples" line, instead of one line per sample.
const SAMPLE_REPORT_INTERVAL: Duration = Duration::from_secs(5);

// Helper to log to a file that X-Plane can access
fn log_to_file(message: &str) {
    use std::fs::OpenOptions;
    use std::io::Write;
    
    let log_path = std::env::temp_dir().join(LOG_NAME);
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
    {
        // Use SystemTime instead of chrono
        if let Ok(elapsed) = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
            let secs = elapsed.as_secs();
            let millis = elapsed.subsec_millis();
            let _ = writeln!(file, "[{}.{:03}] {}", secs, millis, message);
        } else {
            let _ = writeln!(file, "[TIME_ERROR] {}", message);
        }
    }
}

/// Keep the previous run's log as `baton_debug.old.log` and start a fresh one.
///
/// Only one old log is kept, so the files cannot grow past two runs. Does
/// nothing when there is no log yet. Failures are ignored: a log that cannot be
/// rotated just keeps being appended to.
fn rotate_log(dir: &Path) {
    let current = dir.join(LOG_NAME);
    if !current.exists() {
        return;
    }
    let old = dir.join(OLD_LOG_NAME);
    let _ = std::fs::remove_file(&old);
    let _ = std::fs::rename(&current, &old);
}

/// Clears the "connected" flag when the send thread ends, however it ends, so
/// `send()` goes back to dropping samples.
struct ConnectedGuard(Arc<AtomicBool>);

impl Drop for ConnectedGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// How many times the handshake polls for the relay's reply, and how long it
/// waits between tries. A second in total.
const HANDSHAKE_ATTEMPTS: u32 = 100;
const HANDSHAKE_POLL: Duration = Duration::from_millis(10);

/// Wait for the relay's greeting on a non-blocking connection.
///
/// Returns the line it answered with, or None once the tries run out or the
/// read fails outright.
///
/// An empty read is treated as "nothing yet" rather than as the relay hanging
/// up, which is what #254 was. On Windows a non-blocking read of an empty pipe
/// fails with ERROR_NO_DATA; std maps that to BrokenPipe, and interprocess
/// 2.2.2 turns BrokenPipe into Ok(0) before we see it, so a hangup and an empty
/// pipe are the same value here. The relay reads our greeting before it
/// answers, so its reply is never already waiting on the first read, and
/// treating Ok(0) as a hangup meant the plugin gave up every time. A relay that
/// really has gone costs the full second instead of returning at once.
///
/// `buffer` carries over between tries, so a line split across reads is
/// assembled rather than lost.
fn await_greeting<R: BufRead>(
    conn: &mut R,
    buffer: &mut String,
    attempts: u32,
    poll: Duration,
) -> Option<String> {
    for _ in 0..attempts {
        match conn.read_line(buffer) {
            // Only a finished line is the greeting. read_line hands back a
            // partial one as Ok(n), and taking that would both truncate the
            // greeting and leave its tail to be read as telemetry.
            Ok(n) if n > 0 && buffer.ends_with('\n') => return Some(buffer.trim().to_string()),
            // Nothing yet, part of a line, or the relay has gone. The first and
            // the last are the same value on Windows, so all three wait.
            Ok(_) => thread::sleep(poll),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => thread::sleep(poll),
            Err(e) => {
                log_to_file(&format!(
                    "Failed to read server response: {} (kind: {:?})",
                    e,
                    e.kind()
                ));
                return None;
            }
        }
    }
    log_to_file("Timeout waiting for server response");
    None
}

// This defines the interface for the C++ codegen. This is where functions are exposed to the C++ side.
#[cxx::bridge]
mod ffi {
    extern "Rust" {
        // [cxx] Defining a struct in this way makes it opaque on the C++ side; I don't want the C++ side
        // of the code to reach in and mess with my thread handle in any way.
        type Baton;

        fn start(&mut self);

        fn stop(&mut self);

        fn send(&mut self, nums: &CxxVector<f32>);

        fn new_baton_handle() -> Box<Baton>;
    }
}

#[derive(Default)]
pub struct Baton {
    thread: Option<std::thread::JoinHandle<()>>,
    tx: Option<Sender<ChannelSignal>>,
    /// True from the end of the relay handshake until the send thread ends.
    /// While it is false, `send()` drops samples instead of queueing them.
    connected: Arc<AtomicBool>,
    /// Whether the "dropping samples" line has been written for this stretch
    /// without a connection.
    drop_logged: bool,
}

impl Baton {
    pub fn start(&mut self) {
        // Rust does not have nulls. If you do not understand Options, read the Rust Book chapter 6.1
        let None = self.thread else {
            log_to_file("Thread already started!");
            return;
        };

        // A fresh log for this run; the last one is kept as baton_debug.old.log.
        rotate_log(&std::env::temp_dir());
        self.connected.store(false, Ordering::Release);
        self.drop_logged = false;
        let connected = Arc::clone(&self.connected);

        let (tx, rx): (Sender<ChannelSignal>, Receiver<ChannelSignal>) = unbounded();
        self.tx = Some(tx);

        let handle: thread::JoinHandle<_> = thread::spawn(move || {
            // Whichever way this thread ends, send() must stop queueing samples.
            let _guard = ConnectedGuard(Arc::clone(&connected));

            // IMPORTANT: Give the relay time to fully initialize the named pipe listener
            log_to_file("Waiting 2 seconds for relay to initialize...");
            thread::sleep(Duration::from_secs(2));
            
            // OS-dependent abstraction - ensure we use the SAME type as relay
            let name = if GenericNamespaced::is_supported() {
                log_to_file("Using GenericNamespaced (Windows Named Pipe)");
                "baton.sock".to_ns_name::<GenericNamespaced>().unwrap()
            } else {
                log_to_file("Using GenericFilePath (filesystem socket)");
                let mut p = std::env::temp_dir();
                p.push("baton.sock");
                p.to_fs_name::<GenericFilePath>().unwrap()
            };

            // Debug: show the concrete socket name/identifier the baton will try to connect to
            log_to_file(&format!("Attempting to connect to socket: {:?}", name.borrow()));

            // Connection retry loop with an exponential backoff capped at 5 seconds
            let mut retry_delay = Duration::from_millis(500);
            let mut retry_count = 0;
            let conn = loop {
                match Stream::connect(name.borrow()) {
                    Ok(stream) => {
                        log_to_file(&format!("Successfully connected to socket: {:?}", name.borrow()));
                        break stream
                    },
                    Err(e) => {
                        retry_count += 1;
                        log_to_file(&format!("Failed to connect (attempt {}): {} (kind: {:?}). Retrying in {:?}...", 
                            retry_count, e, e.kind(), retry_delay));
                        thread::sleep(retry_delay);
                        retry_delay = (retry_delay * 2).min(Duration::from_secs(5));
                        
                        // Give up after 20 attempts
                        if retry_count >= 20 {
                            log_to_file("Too many connection attempts, giving up");
                            return;
                        }
                    }
                };
            };

            log_to_file("Setting socket to non-blocking mode");
            if let Err(e) = conn.set_nonblocking(true) {
                log_to_file(&format!("Failed to set non-blocking mode: {}", e));
                return;
            }

            // immediately "shadow" the Stream we create, wrapping it in a BufReader.
            // "shadowing" lets you re-use variable names. for more, see the Rust Book chapter 3.1.
            let mut conn = BufReader::new(conn);

            // BufReader doesn't implement the Write Trait, so we use `get_mut()` to obtain
            // a mutable reference to the Stream that the BufReader wraps. We write using that Stream.
            // See the Rust Book chapter 4 if you are unfamiliar with references.
            log_to_file("Sending greeting message");
            if let Err(e) = conn.get_mut().write_all(b"Hello, from the baton prototype (Rust lib called from C++)\n") {
                log_to_file(&format!("Failed to send greeting: {} (kind: {:?})", e, e.kind()));
                return;
            }
            
            if let Err(e) = conn.get_mut().flush() {
                log_to_file(&format!("Failed to flush after greeting: {} (kind: {:?})", e, e.kind()));
                return;
            }

            // read the contents from the stream into the buffer -- we don't need a mutable reference here
            // like above because BufReader implements the Read Trait.
            log_to_file("Waiting for server response");
            let mut buffer = String::with_capacity(128);
            match await_greeting(&mut conn, &mut buffer, HANDSHAKE_ATTEMPTS, HANDSHAKE_POLL) {
                Some(greeting) => log_to_file(&format!("Server answered: {}", greeting)),
                None => return,
            }
            buffer.clear();
            connected.store(true, Ordering::Release);

            // send a bunch of data for the frequency test in one-second intervals
            log_to_file("Starting test data transmission");
            for i in 0..3 {
                for _j in 0..5 {
                    if let Err(e) = conn.get_mut().write_all(b"0\n") {
                        log_to_file(&format!("Test data send error: {} (kind: {:?})", e, e.kind()));
                        return;
                    }
                }
                if let Err(e) = conn.get_mut().flush() {
                    log_to_file(&format!("Test data flush error: {} (kind: {:?})", e, e.kind()));
                    return;
                }
                log_to_file(&format!("Sent test batch {}/3", i + 1));
                thread::sleep(Duration::from_secs(1));
            }

            log_to_file("Entering main send loop");
            // Samples are not logged one by one: at 20 Hz that fills the log in
            // minutes. Count them and report every SAMPLE_REPORT_INTERVAL.
            let mut sent_since_report: u32 = 0;
            let mut last_report = Instant::now();
            // Continuously send values
            loop {
                for message in rx.try_iter() {
                    match message {
                        ChannelSignal::Send(n) => {
                            let s: String = format!("{n}\n");
                            match conn.get_mut().write_all(s.as_bytes()) {
                                Ok(_) => {
                                    match conn.get_mut().flush() {
                                        Ok(_) => {
                                            sent_since_report += 1;
                                        }
                                        Err(e) => {
                                            log_to_file(&format!("Flush error: {} (kind: {:?})", e, e.kind()));
                                            return;
                                        }
                                    }
                                }
                                Err(e) => {
                                    log_to_file(&format!("Send error: {} (kind: {:?})", e, e.kind()));
                                    return;
                                }
                            }
                        }
                        ChannelSignal::Stop => {
                            log_to_file("Received stop signal");
                            let _ = conn.get_mut().write_all("SHUTDOWN\n".as_bytes());
                            let _ = conn.get_mut().flush();
                            return;
                        }
                    }
                }
                if sent_since_report > 0 && last_report.elapsed() >= SAMPLE_REPORT_INTERVAL {
                    log_to_file(&format!(
                        "Sent {} samples in the last {:.1?}",
                        sent_since_report,
                        last_report.elapsed()
                    ));
                    sent_since_report = 0;
                    last_report = Instant::now();
                }
                // Small sleep to avoid busy-spinning and allow OS to schedule writes
                thread::sleep(Duration::from_millis(10));
            }
        });

        self.thread = Some(handle);
    }

    pub fn stop(&mut self) {
        let Some(handle) = self.thread.take() else {
            log_to_file("No currently running thread.");
            return;
        };

        log_to_file("Attempting to stop thread...");
        // Signal the thread to stop operations
        if let Some(tx) = &self.tx {
            let _ = tx.send(ChannelSignal::Stop);
        }

        // Block until thread completes
        let _ = handle.join();

        log_to_file("Thread stopped successfully!");
        self.thread = None;
    }

    fn send(&mut self, nums: &CxxVector<f32>) {
        // No connection to the relay: drop the sample rather than log it or let
        // the queue grow. One line says so; it is written again only if a
        // connection that was up goes away.
        if !self.connected.load(Ordering::Acquire) {
            if !self.drop_logged {
                log_to_file("No connection to the relay: dropping samples until it connects");
                self.drop_logged = true;
            }
            return;
        }
        self.drop_logged = false;

        let s = nums
            .into_iter()
            .fold(String::new(), |acc, num| format!("{acc};{num}"));
        if let Some(tx) = &self.tx {
            let _ = tx.send(ChannelSignal::Send(s));
        }
    }
}

pub fn new_baton_handle() -> Box<Baton> {
    Box::new(Baton::default())
}

enum ChannelSignal {
    Stop,
    Send(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Read};

    /// A reader that hands back a scripted sequence of results, so the
    /// handshake can be driven through the cases a real pipe produces.
    struct ScriptedReader {
        steps: Vec<std::io::Result<&'static str>>,
    }

    impl Read for ScriptedReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.steps.is_empty() {
                return Ok(0);
            }
            match self.steps.remove(0) {
                Ok(text) => {
                    let bytes = text.as_bytes();
                    buf[..bytes.len()].copy_from_slice(bytes);
                    Ok(bytes.len())
                }
                Err(e) => Err(e),
            }
        }
    }

    fn would_block() -> std::io::Result<&'static str> {
        Err(std::io::Error::new(std::io::ErrorKind::WouldBlock, "no data"))
    }

    fn run(steps: Vec<std::io::Result<&'static str>>, attempts: u32) -> Option<String> {
        let mut reader = BufReader::new(ScriptedReader { steps });
        let mut buffer = String::new();
        await_greeting(&mut reader, &mut buffer, attempts, Duration::from_millis(0))
    }

    // #254: on Windows an empty pipe reads as Ok(0), so giving up on it meant
    // the plugin never connected. The relay answers a moment later.
    #[test]
    fn an_empty_read_is_not_a_hangup() {
        let greeting = run(
            vec![Ok(""), Ok(""), Ok(""), Ok("Hello, from the relay (Rust)!\n")],
            100,
        );
        assert_eq!(greeting.as_deref(), Some("Hello, from the relay (Rust)!"));
    }

    #[test]
    fn a_would_block_is_still_retried() {
        let greeting = run(vec![would_block(), Ok("Hello, from the relay (Rust)!\n")], 100);
        assert_eq!(greeting.as_deref(), Some("Hello, from the relay (Rust)!"));
    }

    // A relay that never answers must not hang the thread stop() joins.
    #[test]
    fn a_silent_relay_gives_up_after_the_attempts_run_out() {
        assert_eq!(run(vec![], 5), None);
    }

    #[test]
    fn a_real_read_error_gives_up_at_once() {
        let steps = vec![Err(std::io::Error::new(
            std::io::ErrorKind::ConnectionReset,
            "reset",
        ))];
        assert_eq!(run(steps, 100), None);
    }

    // read_line can return a line in pieces; the buffer carries over.
    #[test]
    fn a_greeting_split_across_reads_is_assembled() {
        let greeting = run(vec![Ok("Hello, from "), Ok(""), Ok("the relay (Rust)!\n")], 100);
        assert_eq!(greeting.as_deref(), Some("Hello, from the relay (Rust)!"));
    }

    /// A scratch folder per test, so the tests can run in parallel.
    fn scratch_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("baton_test_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // #262: each run starts a fresh log and keeps the last one as the old log.
    #[test]
    fn rotating_moves_the_log_aside() {
        let dir = scratch_dir("moves_aside");
        std::fs::write(dir.join(LOG_NAME), "last run").unwrap();
        rotate_log(&dir);
        assert!(!dir.join(LOG_NAME).exists());
        assert_eq!(std::fs::read_to_string(dir.join(OLD_LOG_NAME)).unwrap(), "last run");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Only one old log is kept, so the files cannot grow without limit.
    #[test]
    fn rotating_replaces_the_previous_old_log() {
        let dir = scratch_dir("replaces_old");
        std::fs::write(dir.join(OLD_LOG_NAME), "two runs ago").unwrap();
        std::fs::write(dir.join(LOG_NAME), "last run").unwrap();
        rotate_log(&dir);
        assert_eq!(std::fs::read_to_string(dir.join(OLD_LOG_NAME)).unwrap(), "last run");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rotating_with_no_log_does_nothing() {
        let dir = scratch_dir("no_log");
        rotate_log(&dir);
        assert!(!dir.join(LOG_NAME).exists());
        assert!(!dir.join(OLD_LOG_NAME).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
