mod message;
mod state;
mod update;
mod view;
mod ipc;

use std::thread;

use self::{message::Message, state::State, update::update, view::view, ipc::ipc_connection_loop};

use iced::{
    time::{every, Duration},
    Task,
};

fn main() -> iced::Result {
    // Create initial IPC Connection and Thread
    let (tx_kill, rx_kill) = std::sync::mpsc::channel();
    let (txx, rxx) = std::sync::mpsc::channel();
    let ipc_connection_handle = thread::spawn(move || {
        println!("Initial IPC Connection!");
        ipc_connection_loop(rx_kill, txx)
    });

    iced::application("RELAY", update, view)
        .window_size((250.0, 100.0))
        .exit_on_close_request(false)
        .subscription(subscribe)
        .run_with(|| {
            // for pre-run state initialization
            let state = State {
                elapsed_time: Duration::ZERO,
                ipc_conn_thread_handle: Some(ipc_connection_handle),
                tx_kill: Some(tx_kill),
                rx_baton: Some(rxx),
                latest_baton_send: None,
            };
            (state, Task::none())
        })
}

fn subscribe(_state: &State) -> iced::Subscription<Message> {
    use Message as M;

    // Subscription for displaying elapsed time -- temporary
    let time_sub = every(Duration::from_secs(1)).map(|_| M::Update);

    // Subscription to re-check the baton connection
    let baton_sub = every(Duration::from_millis(10)).map(|_| M::BatonMessage);

    // Subscription to send a message when the window close button (big red X) is clicked.
    // Needed to execute cleanup operations before actually shutting down, such as saving etc
    let window_close = iced::window::close_requests().map(|id| M::WindowCloseRequest(id));

    // combine and return all subscriptions as one subscription to satisfy the return type
    iced::Subscription::batch([time_sub, baton_sub, window_close])
}
