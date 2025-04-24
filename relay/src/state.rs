use anyhow::bail;
use anyhow::Result;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::thread::JoinHandle;

use iced::time::Duration;

use crate::bichannel;
use crate::bichannel::ChildBiChannel;
use crate::bichannel::ParentBiChannel;
use crate::message::FromIpcThreadMessage;
use crate::message::FromTcpThreadMessage;
use crate::message::ToIpcThreadMessage;
use crate::message::ToTcpThreadMessage;

use crate::ChannelMessage;

use interprocess::local_socket::{
    traits::{Listener, ListenerExt},
    GenericNamespaced, ListenerOptions, ToNsName,
};

#[derive(Default)]
#[allow(unused)]
pub(crate) struct State {
    pub elapsed_time: Duration,

    pub ipc_thread_handle: Option<JoinHandle<()>>,
    pub tcp_thread_handle: Option<JoinHandle<()>>,

    pub tx_kill: Option<std::sync::mpsc::Sender<()>>,
    pub rx_baton: Option<std::sync::mpsc::Receiver<FromIpcThreadMessage>>,
    pub connection_status: Option<ChannelMessage>,
    pub latest_baton_send: Option<String>,
    pub active_baton_connection: bool,
    pub recv: Option<std::sync::mpsc::Receiver<ChannelMessage>>,

    pub ipc_bichannel: Option<ParentBiChannel<ToIpcThreadMessage, FromIpcThreadMessage>>,
    pub tcp_bichannel: Option<ParentBiChannel<ToTcpThreadMessage, FromTcpThreadMessage>>,
}

impl State {
    pub fn ipc_connect(&mut self) -> Result<()> {
        if self.ipc_thread_handle.is_some() {
            bail!("IPC thread already exists.")
        }

        // TODO
        let (ipc_bichannel, mut child_bichannel): (
            ParentBiChannel<ToIpcThreadMessage, FromIpcThreadMessage>,
            ChildBiChannel<ToIpcThreadMessage, FromIpcThreadMessage>,
        ) = bichannel::create_bichannels();
        let ipc_thread_handle = std::thread::spawn(move || {
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
                let conn = match (child_bichannel.is_killswitch(), conn) {
                    (true, _) => return,
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
                                let _ = child_bichannel
                                    .send_to_parent(FromIpcThreadMessage::BatonShutdown);
                                break;
                            } else {
                                // actual baton data received
                                let _ = child_bichannel.send_to_parent(
                                    FromIpcThreadMessage::BatonData(buffer.clone()),
                                );
                            }

                            buffer.clear();
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                        Err(e) => panic!("Got err {e}"),
                    }
                }
            }
        });

        self.ipc_bichannel = Some(ipc_bichannel);

        Ok(())
    }

    pub fn tcp_connect(&mut self) -> Result<()> {
        if self.tcp_thread_handle.is_some() {
            bail!("TCP thread already exists.")
        }

        // TODO
        let (tcp_bichannel, child_bichannel) = bichannel::create_bichannels();
        let tcp_thread_handle = std::thread::spawn(move || {});

        self.ipc_bichannel = Some(tcp_bichannel);

        todo!()
    }
}
