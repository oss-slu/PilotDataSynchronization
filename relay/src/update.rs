use iced::{time::Duration, Task};

use crate::{IpcThreadMessage, Message, State};

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
            if let Some(ref tx) = state.tx_kill {
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
            match state.rx_baton.as_ref().and_then(|rx| rx.try_recv().ok()) {
                Some(IpcThreadMessage::BatonData(data)) => {
                    state.latest_baton_send = Some(data);
                    state.active_baton_connection = true;
                }
                Some(IpcThreadMessage::BatonShutdown) => state.active_baton_connection = false,
                None => { /* do nothing */ }
            };
            Task::none()
        }
        _ => Task::none(),
    }
}
