use iced::{time::Duration, Task};

use crate::channel::{FromTcpThreadMessage, IpcThreadMessage, ToTcpThreadMessage};
use crate::{Message, State};

pub(crate) fn update(state: &mut State, message: Message) -> Task<Message> {
    use Message as M;

    #[allow(unreachable_patterns)]
    match message {
        M::Update => {
            state.elapsed_time += Duration::from_secs(1);
            Task::none()
        }
        M::WindowCloseRequest(id) => {
            // pre-shutdown operations go here
            if let Some(ref tx) = state.tx_ipc_to_thread {
                let _ = tx.send(());
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
                .rx_ipc_from_thread
                .as_ref()
                .and_then(|rx| rx.try_recv().ok())
            {
                Some(IpcThreadMessage::BatonData(data)) => {
                    state.latest_baton_send = Some(data.clone());
                    // state.data_from_baton.push_back(data); // TODO: check if dequeue is needed
                    if let Some(rx_tcp_to_thread) = state.tx_tcp_to_thread.as_ref() {
                        // TODO: more exhaustive check
                        let _ = rx_tcp_to_thread.send(ToTcpThreadMessage::Send(data));
                    }

                    // What was this supposed to be? This doesn't reference a field that exists.
                    // state.active_baton_connection = true;
                }
                Some(IpcThreadMessage::BatonShutdown) => {
                    // Again, this references a field that does not exist.
                    // state.active_baton_connection = false;

                    if let Some(handle) = state.ipc_thread_handle.take() {
                        let _ = handle.join();
                    }
                }
                None => { /* do nothing */ }
            };
            Task::none()
        }
        M::ConnectionMessage => {
            println!("Check Connection Status");
            if let Some(status) = state
                .rx_tcp_from_thread
                .as_ref()
                .and_then(|recv| recv.try_recv().ok())
            {
                state.tcp_connection_status = status == FromTcpThreadMessage::SuccessfullyConnected;
            }
            Task::none()
        }
        M::ConnectTcp(address) => {
            state.tcp_connect(&address);
            Task::none()
        }
        M::DisconnectTcp => {
            if let Err(e) = state.tcp_disconnect() {
                state.log_error(e);
            };
            Task::none()
        }
        _ => Task::none(),
    }
}
