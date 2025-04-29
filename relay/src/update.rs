use iced::{time::Duration, Task};

use crate::{Message, State};

use rand::Rng;

pub(crate) fn update(state: &mut State, message: Message) -> Task<Message> {
    use Message as M;

    #[allow(unreachable_patterns)]
    match message {
        M::Update => {
            state.elapsed_time += Duration::from_millis(10);

            

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
        M::ConnectionMessage => {
            println!("Check Connection Status");
            if let Some(status) = state.recv.as_ref().and_then(|recv| recv.try_recv().ok()) {
                state.connection_status = status
            }
            Task::none()
        }

        M::Tick => {
            let mut rng = rand::thread_rng();
            let x: usize = rng.gen_range(1..=5);
            //let y: f32 = rng.gen_range(-1.0..=1.0);
            //std::println!("Updating: {} and {}!", x, y);
            
            state.chart.update();
            state.chart.add(x);

            Task::none()
        }
        _ => Task::none(),
    }
}
