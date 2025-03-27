use iced::{time::Duration, Task};

use crate::{Message, State, ipc::ipc_connection_loop};

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
            if let Some(num) = state.rx_baton.as_ref().and_then(|rx| rx.try_recv().ok()) {
                state.latest_baton_send = Some(num);
            }
            Task::none()
        }
        M::BatonReconnectMessage => {
            println!("Baton Reconnect Message");
            // Kill existing IPC thread
            if let Some(tx_kill) = &state.tx_kill {
                let _ = tx_kill.send(()); 
            }

            // Create new IPC thread
            let (tx_kill, rx_kill) = std::sync::mpsc::channel();
            let (txx, rxx) = std::sync::mpsc::channel();
            let handle = std::thread::spawn(move || {
                ipc_connection_loop(rx_kill, txx);
            });

            // Update state with new IPC thread
            state.ipc_conn_thread_handle = Some(handle);
            state.tx_kill = Some(tx_kill);
            state.rx_baton = Some(rxx);

            Task::none()
        }
        _ => Task::none(),
    }
}
