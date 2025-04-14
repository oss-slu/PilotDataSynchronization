use interprocess::local_socket::traits::{Listener, ListenerExt};

use std::io::{BufRead, BufReader, Write};

use iced::time::Duration;

use crate::IpcThreadMessage;

// Pulled the IPC connection loop out into it's own crate/function
pub(crate) fn ipc_connection_loop(
    listener: &impl Listener,
    rx_kill: std::sync::mpsc::Receiver<()>,
    txx: std::sync::mpsc::Sender<IpcThreadMessage>,
) {
    println!("ipc_connection_loop called!");

    let mut buffer = String::with_capacity(128);
    for conn in listener.incoming() {
        let conn = match (rx_kill.try_recv(), conn) {
            (Ok(()), _) => return,
            (_, Ok(c)) => {
                println!("success");
                c
            }
            (_, Err(e)) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            (_, Err(e)) => {
                eprintln!("Incoming connection failed: {e}");
                continue;
            }
        };

        let mut conn = BufReader::new(conn);
        println!("Incoming connection!");

        // I think this is another connection test? seems redundant with the other stuff going on
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

        // send frequency test -- three seconds of receiving 100,000 dummy inputs per second to check stability
        // TODO: Remove these tests soon
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
                Ok(_) => recvs[idx] += 1,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                _ => panic!(),
            }
        }

        println!("recvs: {recvs:?}");
        buffer.clear();

        // Continuously receive data from Baton (plugin)
        loop {
            match conn.read_line(&mut buffer) {
                Ok(s) if s == 0 || buffer.len() == 0 => {
                    buffer.clear();
                    continue;
                }
                Ok(s) => {
                    let _ = buffer.pop(); // remove trailing newline
                    println!("Got: {buffer} ({s} bytes read)");

                    // txx is the sender half of channel from ipc_connection_handle -> main_gui_thread

                    // TODO: change baton to send strings not floats,
                    // ^ UNABLE TO TEST THIS LOGIC UNTIL THAT HAPPENS

                    // baton shutdown message received. Send shutdown message and break to next connection
                    // if the first 8 letters or so contains "SHUTDOWN",
                    if buffer.starts_with("SHUTDOWN") {
                        let _ = txx.send(IpcThreadMessage::BatonShutdown);
                        break;
                    } else {
                        // actual baton data received
                        let _ = txx.send(IpcThreadMessage::BatonData(buffer.clone()));
                    }

                    buffer.clear();
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(e) => panic!("Got err {e}"),
            }
        }

        println!("Onto next iteration of loop and connection!");
    }
}
