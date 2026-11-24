mod bichannel;
mod events;
mod listener;
mod message;
mod receiver;
mod replay;
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
    iced::application("RELAY", update, view)
        // Taller than it was: the window clipped its own content well before
        // the listener panel was added to it. Wide enough for the iMotions
        // row, which is a button, an address and a dropdown side by side.
        .window_size((640.0, 860.0))
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

            // Asked once, here, so a missing uv is a line in the window rather
            // than a listener that fails to start with a pilot already seated.
            state.uv_available = Some(uv_on_path());

            // The recordings a flight can be rehearsed with. Only their headers
            // are read, so this costs nothing at startup.
            if let Some(inference) = state.inference_dir.clone() {
                state.recordings = replay::recordings_in(&inference.join("data"));
            }

            (state, Task::none())
        })
}

/// Whether `uv` can be run at all.
///
/// `uv --version` rather than looking for the file: PATH resolution on Windows
/// involves PATHEXT and the per-user install location, and asking the system
/// to run it is the only answer that matches what spawning the listener will
/// actually do.
fn uv_on_path() -> bool {
    std::process::Command::new("uv")
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
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
