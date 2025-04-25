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
            close_window(state, id)
        }
        M::BatonMessage => {
            handle_baton_msg(state)
        }

      M::ConnectionMessage => {     
            update_connection_status(state) 
        },
        _ => Task::none(),
    }
}

fn update_connection_status(state: &mut State) -> Task<Message> {
    if let Some(status) = state.recv.as_ref().and_then(|recv| recv.try_recv().ok()){
        state.connection_status = Some(status)
    }           

    Task::none()
}



fn close_window(state: &mut State, id: iced::window::Id) -> Task<Message> {
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


fn handle_baton_msg(state: &mut State) -> Task<Message> {
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


#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;
    use crate::ChannelMessage;

    #[test]
    fn handle_baton_msg_data_unit_test() {
        let (txx, rxx) = mpsc::channel();
        txx.send(IpcThreadMessage::BatonData("test".into())).unwrap();
    
        let mut state = State {
            elapsed_time: Duration::ZERO,
            ipc_conn_thread_handle: None,
            tx_kill: None,
            rx_baton: Some(rxx),
            latest_baton_send: None,
            recv: None,
            connection_status: None,
            active_baton_connection: false,
        };
    
        let _ = handle_baton_msg(&mut state);
    
        assert_eq!(state.latest_baton_send, Some("test".into()));
        assert!(state.active_baton_connection);
    }
    
    
    #[test]
    fn handle_baton_msg_shutdown_unit_test() {
        let (txx, rxx) = mpsc::channel();
        txx.send(IpcThreadMessage::BatonShutdown).unwrap();
    
        let mut state = State {
            elapsed_time: Duration::ZERO,
            ipc_conn_thread_handle: None,
            tx_kill: None,
            rx_baton: Some(rxx),
            latest_baton_send: None,
            recv: None,
            connection_status: None,
            active_baton_connection: true,
        };
    
        let _ = handle_baton_msg(&mut state);
    
        assert_eq!(state.latest_baton_send, Some("SHUTDOWN".into()));
        assert!(!state.active_baton_connection);
    }
    
    #[test]
    fn update_connection_state_true() {
        let (send, recv) = std::sync::mpsc::channel::<ChannelMessage>();
        send.send(ChannelMessage::Connect);    
    
        let mut state = State {
            elapsed_time: Duration::ZERO,
            ipc_conn_thread_handle: None,
            tx_kill: None,
            rx_baton: None,
            latest_baton_send: None,
            recv: Some(recv),
            connection_status: None,
            active_baton_connection: false,
        };

        let _ = update_connection_status(&mut state);

        match state.connection_status {
            Some(ChannelMessage::Connect) => println!("yay"),
            Some(ChannelMessage::Disconnected) => println!("boo"),
            None => println!("tf u doin")
        };

        assert_eq!(state.connection_status, Some(ChannelMessage::Connect));

    }




    // TODO! 
    #[test]
    fn close_window_unit_test() {
        let mut state = State {
            elapsed_time: Duration::ZERO,
            ipc_conn_thread_handle: None,
            tx_kill: None,
            rx_baton: None,
            latest_baton_send: None,
            recv: None,
            connection_status: None,
            active_baton_connection: false,
        };

        let id = iced::window::Id::unique();
        let window_close: Task<Message> = iced::window::close(id);

        let result = close_window(&mut state, id);

        todo!()
        //assert_eq!(result, window_close);
    }

}