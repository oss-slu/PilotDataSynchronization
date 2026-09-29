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
    thread,
    time::{Duration, SystemTime},
};

// Helper to log to a file that X-Plane can access
fn log_to_file(message: &str) {
    use std::fs::OpenOptions;
    use std::io::Write;
    
    let log_path = std::env::temp_dir().join("baton_debug.log");
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
}

impl Baton {
    pub fn start(&mut self) {
        // Rust does not have nulls. If you do not understand Options, read the Rust Book chapter 6.1
        let None = self.thread else {
            log_to_file("Thread already started!");
            return;
        };

        let (tx, rx): (Sender<ChannelSignal>, Receiver<ChannelSignal>) = unbounded();
        self.tx = Some(tx);

        let handle: thread::JoinHandle<_> = thread::spawn(move || {
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
            let mut attempts = 0;
            loop {
                match conn.read_line(&mut buffer) {
                    Ok(0) => {
                        log_to_file("Server closed connection during handshake");
                        return;
                    }
                    Ok(_) => {
                        log_to_file(&format!("Server answered: {}", buffer.trim()));
                        break;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        attempts += 1;
                        if attempts > 100 {
                            log_to_file("Timeout waiting for server response");
                            return;
                        }
                        thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                    Err(e) => {
                        log_to_file(&format!("Failed to read server response: {} (kind: {:?})", e, e.kind()));
                        return;
                    }
                }
            }
            buffer.clear();

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
                                            log_to_file(&format!("Sent: {}", s.trim()));
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
        let s = nums
            .into_iter()
            .fold(String::new(), |acc, num| format!("{acc};{num}"));
        log_to_file(&format!("Attempted to send {s}"));
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
