/*
This file is the heart of this Rust submodule. The crate in use, cxx, is used to create a safe interop
layer between Rust and C++. See the high level overview in the top-level project root README.md for details
on why we want this.

Keep in mind that what is shown here is not entirely typical of Rust code, as this crate takes liberties
in order to facilitate the aforementioned interoperability.
*/

use core::time::Duration;
use iceoryx2::prelude::*;
use std::thread;

const CYCLE_TIME: Duration = Duration::from_secs(1);

// This defines the interface for the C++ codegen. This is where functions are exposed to the C++ side.
#[cxx::bridge]
mod ffi {
    extern "Rust" {
        // [cxx] Defining a struct in this way makes it opaque on the C++ side; I don't want the C++ side
        // of the code to reach in and mess with my thread handle in any way.
        type ThreadWrapper;

        fn start(&mut self);

        fn stop(&mut self);

        fn new_wrapper() -> Box<ThreadWrapper>;
    }
}

#[derive(Default)]
pub struct ThreadWrapper {
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ThreadWrapper {
    pub fn start(&mut self) {
        let None = self.thread else {
            println!("Thread already started!");
            return;
        };

        let handle: thread::JoinHandle<_> = thread::spawn(|| {
            // println!("Hello, from the spawned Rust thread!");
            let node = NodeBuilder::new().create::<ipc::Service>().unwrap();

            let service = node
                .service_builder(&"IPC/Test".try_into().unwrap())
                .publish_subscribe::<u64>()
                .open_or_create()
                .unwrap();

            let publisher = service.publisher_builder().create().unwrap();

            let mut count = 0;
            while node.wait(CYCLE_TIME).is_ok() && count < 5 {
                let sample = publisher.loan_uninit().unwrap();
                let sample = sample.write_payload(1234);
                sample.send().unwrap();
                println!("Sent!");
                count += 1;
            }
        });

        self.thread = Some(handle);
    }

    pub fn stop(&mut self) {
        let Some(handle) = self.thread.take() else {
            println!("No currently running thread.");
            return;
        };

        let _ = handle.join();
        println!("Thread stopped successfully!");
        self.thread = None;
    }
}

pub fn new_wrapper() -> Box<ThreadWrapper> {
    Box::new(ThreadWrapper::default())
}
