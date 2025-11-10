// Library entry for the relay crate. Exposes run_app() used by the binary.
mod bichannel;
mod message;
mod state;
mod update;
mod view;

use message::{FromIpcThreadMessage, Message};
use state::State;
use update::update;
use view::view;

use iced::time::{every, Duration};
use iced::Task;

pub fn run_app() -> iced::Result {
    iced::application("RELAY", update, view)
        .window_size((450.0, 300.0))
        .exit_on_close_request(false)
        .subscription(subscribe)
        .run_with(|| {
            // for pre-run state initialization
            let mut state = State {
                ..Default::default()
            };

            if let Err(e) = state.ipc_connect() {
                state.event_log.push(format!(
                    "Error connecting to IPC during GUI initialization: {e:?}"
                ));
            };

            (state, Task::none())
        })
}

fn subscribe(_state: &State) -> iced::Subscription<Message> {
    use Message as M;

    // Subscription for displaying elapsed time -- temporary
    let time_sub = every(Duration::from_millis(10)).map(|_| M::Update);

    // Subscription to send a message when the window close button is clicked.
    let window_close = iced::window::close_requests().map(|id| M::WindowCloseRequest(id));

    iced::Subscription::batch([time_sub, window_close])
}