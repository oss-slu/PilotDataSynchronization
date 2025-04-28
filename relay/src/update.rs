use iced::{time::Duration, Task};

use crate::{message::ToTcpThreadMessage, FromIpcThreadMessage, Message, State};

pub(crate) fn update(state: &mut State, message: Message) -> Task<Message> {
    use Message as M;

    #[allow(unreachable_patterns)]
    match message {
        M::Update => {
            state.elapsed_time += Duration::from_secs(1);

            // check for messages from IPC thread
            if let Some(ipc_bichannel) = &state.ipc_bichannel {
                for message in ipc_bichannel.received_messages() {
                    match message {
                        FromIpcThreadMessage::BatonData(data) => {
                            state.tcp_bichannel.as_mut().map(|tcp_bichannel| {
                                tcp_bichannel.send_to_child(ToTcpThreadMessage::Send(data))
                            });
                        }
                        FromIpcThreadMessage::BatonShutdown => {
                            state
                                .tcp_disconnect()
                                .expect("Error attempting to shut down TCP server");
                        }
                        _ => (),
                    }
                }
            }

            // check for messages from TCP thread
            if let Some(tcp_bichannel) = &state.tcp_bichannel {
                for message in tcp_bichannel.received_messages() {
                    match message {
                        _ => (),
                    }
                }
            }

            Task::none()
        }
        M::WindowCloseRequest(id) => {
            // pre-shutdown operations go here
            if let Some(ref bichannel) = state.ipc_bichannel {
                let _ = bichannel.killswitch_engage();
            }

            if let Some(ref bichannel) = state.tcp_bichannel {
                let _ = bichannel.killswitch_engage();
            }

            // delete socket file
            let socket_file_path = if cfg!(target_os = "macos") {
                "/tmp/baton.sock"
            } else {
                // TODO: add branch for Windows; mac branch is just for testing/building
                panic!(
                    "No implementation available for given operating system: {}",
                    std::env::consts::OS
                )
            };
            std::fs::remove_file(socket_file_path).unwrap();

            // necessary to actually shut down the window, otherwise the close button will appear to not work
            iced::window::close(id)
        }
        M::BatonMessage => {
            // if we get a message from the ipc_connection_thread, do something with it
            match state
                .ipc_bichannel
                .as_ref()
                .and_then(|bichannel| bichannel.try_recv().ok())
            {
                Some(FromIpcThreadMessage::BatonData(data)) => {
                    state.latest_baton_send = Some(data);
                    state.active_baton_connection = true;
                }
                Some(FromIpcThreadMessage::BatonShutdown) => state.active_baton_connection = false,
                _ => { /* do nothing */ }
            };
            Task::none()
        }

        M::ConnectionMessage => {
            if let Some(status) = state
                .tcp_bichannel
                .as_ref()
                .and_then(|bichannel| bichannel.is_conn_to_endpoint().ok())
            {
                state.tcp_connected = status
            } else {
                state.tcp_connected = false
            }
            Task::none()
        }

        M::ConnectIpc => {
            if let Err(e) = state.ipc_connect() {
                state.log_event(format!("Error: {e:?}"));
            };
            Task::none()
        }
        M::DisconnectIpc => {
            if let Err(e) = state.ipc_disconnect() {
                state.log_event(format!("Error: {e:?}"));
            };
            Task::none()
        }
        M::ConnectTcp => {
            let address = state.tcp_addr_field.clone();
            if let Err(e) = state.tcp_connect(address) {
                state.log_event(format!("Error: {e:?}"));
            };
            Task::none()
        }
        M::DisconnectTcp => {
            if let Err(e) = state.tcp_disconnect() {
                state.log_event(format!("Error: {e:?}"));
            };
            Task::none()
        }
        M::TcpAddrFieldUpdate(addr) => {
            // Update the TCP address text input in the GUI
            state.tcp_addr_field = addr;
            Task::none()
        }
        _ => Task::none(),
    }
}
