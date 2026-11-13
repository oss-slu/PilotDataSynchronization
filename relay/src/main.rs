mod bichannel;
mod cli;
mod message;
mod state;
mod update;
mod view;

use self::{
    message::Message,
    state::State,
    update::update,
    view::view,
};

use iced::{
    time::{every, Duration},
    Task,
};

fn main() -> iced::Result {
    // Before iced starts, so a mistake is reported in the console rather than
    // after a window has opened.
    let outputs = match cli::parse_args(std::env::args().skip(1)) {
        Ok(outputs) => outputs,
        Err(e) if e.downcast_ref::<cli::HelpRequested>().is_some() => {
            println!("{}", cli::USAGE);
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("relay: {e}\n\n{}", cli::USAGE);
            std::process::exit(2);
        }
    };

    iced::application("RELAY", update, view)
        .window_size((450.0, 300.0))
        .exit_on_close_request(false)
        .subscription(subscribe)
        .run_with(|| {
            // for pre-run state initialization
            let mut state = State {
                ..Default::default()
            };

            if let Err(e) = state.load_saved_tcp_addrs() {
                state.log_event(format!("Could not load IP, did it change?: {e:?}"));
            }

            if let Err(e) = state.ipc_connect() {
                state.log_event(format!(
                    "Error connecting to IPC during GUI initialization: {e:?}"
                ));
            };

            // Nothing happens here when no options were given.
            state.keep_connected(outputs);
            (state, Task::none())
        })
}

fn subscribe(_state: &State) -> iced::Subscription<Message> {
    use Message as M;

    // Subscription for displaying elapsed time
    let time_sub = every(Duration::from_millis(10)).map(|_| M::Update);

    // Subscription to send a message when the window close button (big red X) is clicked.
    let window_close = iced::window::close_requests().map(|id| M::WindowCloseRequest(id));

    // combine and return all subscriptions as one subscription to satisfy the return type
    iced::Subscription::batch([time_sub, window_close])
}
