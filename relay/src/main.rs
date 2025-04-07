mod ipc;
mod message;
mod state;
mod update;
mod view;

use std::thread;

use self::{
    ipc::ipc_connection_loop,
    message::{IpcThreadMessage, Message},
    state::State,
    update::update,
    view::view,
};

use iced::{
    time::{every, Duration},
    Task, Theme,
};

use std::sync::mpsc::{Receiver, Sender};

use interprocess::local_socket::{traits::Listener, GenericNamespaced, ListenerOptions, ToNsName};

fn main() -> iced::Result {
    // Communication channels between the main_gui_thread and the ipc_connection_thread
    // tx_kill = transmit FROM main_gui_thread TO ipc_thread
    // named txx_kill because the only thing it does rn is send a kill message to the thread. Can be renamed
    let (tx_kill, rx_kill) = std::sync::mpsc::channel();
    // txx = transmit FROM ipc_thread TO main_gui_thread
    let (txx, rxx): (Sender<IpcThreadMessage>, Receiver<IpcThreadMessage>) =
        std::sync::mpsc::channel();

    // IPC connection Thread
    let ipc_connection_handle = thread::spawn(move || {
        println!("Initial IPC Connection!");

        // Create new IPC Socket Listener builder
        let printname = "baton.sock";
        let name = printname.to_ns_name::<GenericNamespaced>().unwrap();
        let opts = ListenerOptions::new().name(name);

        // Create the actual IPC Socket Listener
        let listener = match opts.create_sync() {
            Ok(x) => x,
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                eprintln!(
                    "Error: could not start server because the socket file is occupied. Please check if {printname} is in use by another process and try again."
                );
                return;
            }
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                eprintln!("Error: could not start server because the OS denied permission: \n{e}");
                return;
            }
            Err(e) => {
                eprintln!("Other Error: {e}");
                return;
            }
        };

        listener
            .set_nonblocking(interprocess::local_socket::ListenerNonblockingMode::Both)
            .expect("Error setting non-blocking mode on listener");

        eprintln!("Server running at {printname}\n");

        // Run the connection loop with the created socket listener
        ipc_connection_loop(&listener, rx_kill, txx)
    });

    iced::application("RELAY", update, view)
        .window_size((400.0, 200.0))
        .exit_on_close_request(false)
        .subscription(subscribe)
        .theme(theme)
        .run_with(|| {
            // for pre-run state initialization
            let state = State {
                elapsed_time: Duration::ZERO,
                ipc_conn_thread_handle: Some(ipc_connection_handle),
                tx_kill: Some(tx_kill),
                rx_baton: Some(rxx),
                latest_baton_send: None,
                active_baton_connection: false,
            };
            (state, Task::none())
        })
}

fn theme(_state: &State) -> Theme {
    Theme::TokyoNight
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
