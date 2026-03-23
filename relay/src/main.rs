mod bichannel;
// mod ipc;
mod message;
mod state;
mod update;
mod view;

// mod channel;
// use channel::ChannelMessage;

use self::{
    // ipc::ipc_connection_loop,
    message::{FromIpcThreadMessage, Message},
    state::State,
    update::update,
    view::view,
};

use iced::{
    time::{every, Duration},
    Task,
};

fn main() -> iced::Result {
    // Communication channels between the main_gui_thread and the ipc_connection_thread
    // tx_kill = transmit FROM main_gui_thread TO ipc_thread
    // named txx_kill because the only thing it does rn is send a kill message to the thread. Can be renamed
    //tcp connection

    // Connect to the server

    /* let (tx_to_parent_thread, rx_from_tcp_thread) = std::sync::mpsc::channel::<ChannelMessage>();
    let tcp_connection = thread::spawn(move || match TcpStream::connect("127.0.0.1:7878") {
        Ok(mut stream) => {
            println!("Successfully connected.");
            let message = ChannelMessage::Connect;
            tx_to_parent_thread.send(message);
        }

        Err(e) => {
            println!("Connection failed: {}", e);
        }
    }); */

    // let (tx_to_ipc_thread, rx_kill) = std::sync::mpsc::channel();
    // let (tx_to_parent_thread, rx_from_parent_thread) = std::sync::mpsc::channel();
    // let _ = tx.send(()); // temp

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
            // state.tcp_connect().expect("TCP connection failure"); // may not need to panic, recoverable error

            (state, Task::none())
        })
}

fn subscribe(_state: &State) -> iced::Subscription<Message> {
    use Message as M;

    // Subscription for displaying elapsed time -- temporary
    let time_sub = every(Duration::from_millis(10)).map(|_| M::Update);

    // Subscription to send a message when the window close button (big red X) is clicked.
    // Needed to execute cleanup operations before actually shutting down, such as saving etc
    let window_close = iced::window::close_requests().map(|id| M::WindowCloseRequest(id));

    // combine and return all subscriptions as one subscription to satisfy the return type
    iced::Subscription::batch([time_sub, window_close])
}
