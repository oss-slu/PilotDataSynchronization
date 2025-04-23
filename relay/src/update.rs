use iced::{time::Duration, Task};

use crate::{FromIpcThreadMessage, Message, State};

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
            // std::fs::remove_file("/tmp/baton.sock").unwrap();
            std::fs::remove_file(socket_file_path).unwrap();

            // necessary to actually shut down the window, otherwise the close button will appear to not work
            iced::window::close(id)
        }
        M::BatonMessage => {
            if let Some(FromIpcThreadMessage::BatonData(s)) =
                state.rx_baton.as_ref().and_then(|rx| rx.try_recv().ok())
            {
                state.latest_baton_send = Some(s);
            }
            Task::none()
        }
        M::ConnectionMessage => {
            println!("Check Connection Status");
            if let Some(status) = state.recv.as_ref().and_then(|recv| recv.try_recv().ok()) {
                state.connection_status = Some(status)
            }
            Task::none()
        }
        _ => Task::none(),
    }
}
