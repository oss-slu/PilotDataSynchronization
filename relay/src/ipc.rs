use interprocess::local_socket::{
    traits::{Listener, ListenerExt},
    GenericNamespaced, ListenerOptions, ToNsName,
};

use std::io::{BufRead, BufReader, Write};

use iced::time::Duration;

/*
Retry connection clarification: 
- Relay should not restart the connection from our side manually 
    --> only autodetect and drop the current connection using the same logic we have now
- if a new connection is attempted by xplane, then relay needs to be able to handle that connection. 
- MAYBE: if new conn detected, then try to write to baton, then if fail drop the connection
- MAYBE: pass a shutdown/reconnect message from baton to relay?
- really, the only connection loop i need to call again is the "for conn in listener.incoming()" loop
*/




// Pulled the IPC connection loop out into it's own crate/function
pub (crate) fn ipc_connection_loop(rx_kill: std::sync::mpsc::Receiver<()>, txx: std::sync::mpsc::Sender<f32>) {
    println!("ipc_connection_loop called!");

    // sample pulled directly from `interprocess` documentation
    let printname = "baton.sock";
    let name = printname.to_ns_name::<GenericNamespaced>().unwrap();
    let opts = ListenerOptions::new().name(name);

    let listener = match opts.create_sync() {
        Ok(x) => x,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            eprintln!(
                "Error: could not start server because the socket file is occupied. Please check if {printname} is in use by another process and try again."
            );
            return;
        },
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!("Error: could not start server because the OS denied permission. This error is currently being workshopped: \n{e}");
            return;
        },
        Err(e) => {
            eprintln!("Other Error: {e}");
            return;
        },
    };

    listener
        .set_nonblocking(interprocess::local_socket::ListenerNonblockingMode::Both)
        .expect("Error setting non-blocking mode on listener");

    eprintln!("Server running at {printname}\n");

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

        // What is the purpose of these indented statements? I am confused
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
                /* Ok(0) => {
                    println!("Termination signal received from baton");
                    continue;
                } */
                Ok(_) => recvs[idx] += 1,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                _ => panic!(),
            }
        }

        println!("recvs: {recvs:?}");
        buffer.clear();

        // Continuously receive data from plugin
        loop {
            match conn.read_line(&mut buffer) {
                // TODO: Create display in GUI for this instead of printing to stdout. Just doing this for ease for the
                // demo for the time being.
                Ok(s) if s == 0 || buffer.len() == 0 => {
                    buffer.clear();
                    continue;
                }
                Ok(s) => {
                    let _ = buffer.pop();   // remove trailing newline
                    println!("Got: {buffer} ({s} bytes read)");
                    if let Ok(num) = buffer.parse::<f32>() {
                        let _ = txx.send(num);
                    }
                    buffer.clear();
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(e) => panic!("Got err {e}"),
            }
        }
    }
}
