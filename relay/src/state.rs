use std::{
    collections::VecDeque,
    io::Write,
    sync::{
        mpsc::{channel, Receiver, Sender},
        Arc, Mutex,
    },
    thread::{sleep, JoinHandle},
};

use iced::time::Duration;

use crate::{
    channel::{FromTcpThreadMessage, IpcThreadMessage},
    ToTcpThreadMessage,
};

use anyhow::{bail, Result};

#[derive(Default)]
#[allow(unused)]
pub(crate) struct State {
    pub elapsed_time: Duration,
    pub ipc_thread_handle: Option<JoinHandle<()>>,
    pub tcp_thread_handle: Option<JoinHandle<()>>,
    pub tcp_kill_flag: Arc<Mutex<bool>>,
    pub tx_ipc_to_thread: Option<Sender<()>>,
    pub rx_ipc_from_thread: Option<Receiver<IpcThreadMessage>>,
    pub tcp_connection_status: bool,
    pub latest_baton_send: Option<String>,
    pub tx_tcp_to_thread: Option<Sender<ToTcpThreadMessage>>,
    pub rx_tcp_from_thread: Option<Receiver<FromTcpThreadMessage>>,
    pub data_from_baton: VecDeque<String>,
    pub error_log: Vec<String>,
}

impl State {
    pub fn tcp_connect(&mut self, address: &str) {
        // early return if already connected; use disconnect_tcp if the tcp server needs to be killed
        if self.tcp_thread_handle.is_some() {
            return;
        }

        // Connect to the server
        let (tx_tcp_to_main, rx_tcp_from_thread) = channel::<FromTcpThreadMessage>();
        let (tx_tcp_to_thread, rx_tcp_from_main) = channel::<ToTcpThreadMessage>();
        self.tx_tcp_to_thread = Some(tx_tcp_to_thread);
        self.rx_tcp_from_thread = Some(rx_tcp_from_thread);

        // Address needs to be moved into the tcp thread context
        let address = address.to_string();

        let kill_flag = self.tcp_kill_flag.clone();

        let tcp_thread_handle = std::thread::spawn(move || {
            let mut stream = loop {
                // continuously attempt to connect until success or kill signal received
                match rx_tcp_from_main.try_recv() {
                    Ok(ToTcpThreadMessage::Disconnect) => return,
                    _ => (),
                }

                match std::net::TcpStream::connect(address.clone()) {
                    Ok(stream) => {
                        println!("Successfully connected.");
                        let _ = tx_tcp_to_main.send(FromTcpThreadMessage::SuccessfullyConnected);
                        break stream;
                    }

                    Err(_) => {
                        // TODO: More thorough error handling by std::io::ErrorKind
                        sleep(Duration::from_secs(3));
                        continue;
                    }
                }
            };

            // Continuously send in a loop until it is time to break;
            loop {
                // Lock the mutex to determine if we need to exit this loop
                // TODO: more exhaustive error checking
                match kill_flag.lock().map(|guard| *guard) {
                    Ok(guard) if guard => return,
                    Err(_) => return,
                    _ => (),
                }

                // retrieve latest data from baton
                // TODO: finish exhaustive checks
                match rx_tcp_from_main.try_recv() {
                    Ok(ToTcpThreadMessage::Send(s)) => {
                        // TODO: proper write check on result
                        let _ = stream.write_all(s.as_bytes());
                    }
                    _ => (),
                }
            }
        });

        self.tcp_thread_handle = Some(tcp_thread_handle);
    }

    pub fn tcp_disconnect(&mut self) -> Result<()> {
        let Some(handle) = self.tcp_thread_handle.take() else {
            bail!("No TCP thread handle currently exists.");
        };

        let Some((tx_tcp_to_thread, _rx_tcp_from_thread)) = self
            .tx_tcp_to_thread
            .take()
            .zip(self.rx_tcp_from_thread.take())
        else {
            bail!(
                "Channel to and from extant TCP thread should also exist, but could not be found."
            );
        };

        // Send kill signal
        tx_tcp_to_thread.send(ToTcpThreadMessage::Disconnect)?;

        // Block to wait on TCP handle shutdown
        if handle.join().is_err() {
            bail!("Error attempting to join TCP handle");
        }

        // We called 'take' on the Options for the thread handle, its sender, and its receiver, so all should be None
        // values in State.
        self.tcp_connection_status = false;

        Ok(())
    }

    pub fn log_error<T: ToString>(&mut self, err: T) {
        self.error_log.push(err.to_string())
    }
}
