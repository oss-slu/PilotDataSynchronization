/*
This file is the heart of this Rust submodule. The crate in use, cxx, is used to create a safe interop
layer between Rust and C++. See the high level overview in the top-level project root README.md for details
on why we want this.

Keep in mind that what is shown here is not entirely typical of Rust code, as this crate takes liberties
in order to facilitate the aforementioned interoperability.
*/

use crossbeam::channel::{unbounded, Receiver, Sender};
use interprocess::local_socket::{
    prelude::*, GenericFilePath, GenericNamespaced, NameType, Stream, ToFsName,
};
use std::{
    io::{prelude::*, BufReader},
    thread,
    time::Duration
};

// This defines the interface for the C++ codegen. This is where functions are exposed to the C++ side.
#[cxx::bridge]
mod ffi {
    extern "Rust" {
        // [cxx] Defining a struct in this way makes it opaque on the C++ side; I don't want the C++ side
        // of the code to reach in and mess with my thread handle in any way.
        type ThreadWrapper;

        fn start(&mut self);

        fn stop(&mut self);

        fn send(&mut self, num: f32);

        fn new_wrapper() -> Box<ThreadWrapper>;
    }
}

#[derive(Default)]
pub struct ThreadWrapper {
    thread: Option<std::thread::JoinHandle<()>>,
    tx: Option<Sender<ChannelSignal>>,
}

impl ThreadWrapper {
    pub fn start(&mut self) {
        // Rust does not have nulls. If you do not understand Options, read the Rust Book chapter 6.1
        let None = self.thread else {
            println!("Thread already started!");
            return;
        };

        let (tx, rx): (Sender<ChannelSignal>, Receiver<ChannelSignal>) = unbounded();
        self.tx = Some(tx);

        let handle: thread::JoinHandle<_> = thread::spawn(move || {
            // OS-dependent abstraction
            let name = if GenericNamespaced::is_supported() {
                "baton.sock".to_ns_name::<GenericNamespaced>().unwrap()
            } else {
                "baton.sock".to_fs_name::<GenericFilePath>().unwrap()
            };

            // used for read/write operations
            let mut buffer = String::with_capacity(128);

            // Connection retry loop with an exponential backoff capped at 5 seconds
            let conn;
            let mut retry_delay = Duration::from_millis(100);
            loop {
                match Stream::connect(name.borrow()) {
                    Ok(stream) => {
                        conn = stream;
                        break;
                    },
                    Err(e) => {
                        println!("Failed to connect: {e}. Retrying in {:?}...", retry_delay);
                        thread::sleep(retry_delay);
                        retry_delay = (retry_delay * 2).min(Duration::from_secs(5));
                    },
                };
            }
            // immediately "shadow" the Stream we create, wrapping it in a BufReader.
            // "shadowing" lets you re-use variable names. for more, see the Rust Book chapter 3.1.
            conn.set_nonblocking(true).unwrap();
            let mut conn = BufReader::new(conn);

            // BufReader doesn't implement the Write Trait, so we use `get_mut()` to obtain
            // a mutable reference to the Stream that the BufReader wraps. We write using that Stream.
            // See the Rust Book chapter 4 if you are unfamiliar with references.
            conn.get_mut()
                .write_all(b"Hello, from the baton prototype (Rust lib called from C++)\n")
                .unwrap();

            // read the contents from the stream into the buffer -- we don't need a mutable reference here
            // like above because BufReader implemenets the Read Trait.
            conn.read_line(&mut buffer).unwrap();

            print!("[RUST] Server answered: {buffer}");

            // send a bunch of data for the frequency test in one-second intervals
            for _ in 0..3 {
                for _ in 0..5 {
                    let _ = conn.get_mut().write_all(b"0\n");
                }
                std::thread::sleep(std::time::Duration::from_secs(1));
            }

            // Continuously send values
            loop {
                for message in rx.try_iter() {
                    let _ = match message {
                        ChannelSignal::Send(n) => {
                            let s: String = format!("{n}\n");
                            let _ = conn.get_mut().write_all(s.as_bytes());
                        }
                        ChannelSignal::Stop => return,
                    };
                }
            }
        });

        self.thread = Some(handle);
    }

    pub fn stop(&mut self) {
        let Some(handle) = self.thread.take() else {
            println!("[RUST] No currently running thread.");
            return;
        };

        println!("[RUST] Attempting to stop thread...");
        // Signal the thread to stop operations
        if let Some(tx) = &self.tx {
            let _ = tx.send(ChannelSignal::Stop);
        }

        // Block until thread completes
        let _ = handle.join();

        println!("[RUST] Thread stopped successfully!");
        self.thread = None;
    }

    fn send(&mut self, num: f32) {
        println!("[RUST] Attempted to send {num:?}");
        if let Some(tx) = &self.tx {
            let _ = tx.send(ChannelSignal::Send(num));
        }
    }
}

pub fn new_wrapper() -> Box<ThreadWrapper> {
    Box::new(ThreadWrapper::default())
}

enum ChannelSignal {
    Stop,
    Send(f32),
}
