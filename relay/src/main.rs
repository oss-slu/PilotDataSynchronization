mod message;
mod state;
mod update;
mod view;

use std::{
    io::{BufRead, BufReader, Write},
    thread,
};

use self::{message::Message, state::State, update::update, view::view};

use iced::{
    time::{every, Duration},
    Task,
};
use interprocess::local_socket::{
    traits::{Listener, ListenerExt},
    GenericNamespaced, ListenerOptions, ToNsName,
};

fn main() -> iced::Result {
    let (tx, rx) = std::sync::mpsc::channel();
    // let _ = tx.send(()); // temp
    let handle = thread::spawn(move || {
        // sample pulled directly from `interprocess` documentation

        let printname = "baton.sock";
        let name = printname.to_ns_name::<GenericNamespaced>().unwrap();

        let opts = ListenerOptions::new().name(name);

        let listener = match opts.create_sync() {
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                eprintln!(
                    "Error: could not start server because the socket file is occupied. Please check if 
                    {printname} is in use by another process and try again."
                );
                return;
            }
            x => x.unwrap(),
        };
        listener
            .set_nonblocking(interprocess::local_socket::ListenerNonblockingMode::Both)
            .expect("Error setting non-blocking mode on listener");

        eprintln!("Server running at {printname}");

        let mut buffer = String::with_capacity(128);

        for conn in listener.incoming() {
            let conn = match (rx.try_recv(), conn) {
                (Ok(()), _) => return,
                (_, Ok(c)) => {
                    println!("success");
                    c
                }
                (_, Err(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    continue;
                }
                (_, Err(e)) => {
                    eprintln!("Incoming connection failed: {e}");
                    continue;
                }
            };

            let mut conn = BufReader::new(conn);
            println!("Incoming connection!");

            match conn.read_line(&mut buffer) {
                Ok(_) => (),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                _ => panic!(),
            }

            let write_res = conn
                .get_mut()
                .write_all(b"Hello, from the relay prototype (Rust)!\n");

            match write_res {
                Ok(_) => (),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                _ => panic!(),
            }

            print!("Client answered: {buffer}");

            buffer.clear();

            // send frequency test
            println!("beginning frequency test...");
            let start = std::time::Instant::now();
            let mut recvs = vec![0, 0, 0];
            loop {
                let elapsed = std::time::Instant::now() - start;
                let idx = match elapsed {
                    dur if dur < Duration::from_secs(1) => 0,
                    dur if dur < Duration::from_secs(2) => 1,
                    dur if dur < Duration::from_secs(3) => 2,
                    _ => break,
                };
                match conn.read_line(&mut buffer) {
                    Ok(0) => continue,
                    Ok(_) => recvs[idx] += 1,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                    _ => panic!(),
                }
            }

            println!("recvs: {recvs:?}");

            break;
        }
    });

    iced::application("RELAY", update, view)
        .exit_on_close_request(false)
        .subscription(subscribe)
        .run_with(|| {
            // for pre-run state initialization
            let state = State {
                elapsed_time: Duration::ZERO,
                flicker: false,
                thread_handle: Some(handle),
                tx: Some(tx),
            };
            (state, Task::none())
        })
}

fn subscribe(_state: &State) -> iced::Subscription<Message> {
    use Message as M;

    let time_sub = every(Duration::from_secs(1)).map(|_| M::Update);
    let flicker_sub = every(Duration::from_millis(500)).map(|_| M::Flicker);

    // Subscription to send a message when the window close button (big red X) is clicked.
    // Needed to execute cleanup operations before actually shutting down, such as saving etc
    let window_close = iced::window::close_requests().map(|id| M::WindowCloseRequest(id));

    // combine and return all subscriptions as one subscription to satisfy the return type
    iced::Subscription::batch([time_sub, flicker_sub, window_close])
}
