use std::thread::JoinHandle;

use iced::time::Duration;

use crate::IpcThreadMessage;

use crate::ChannelMessage;

#[derive(Default)]
#[allow(unused)]
pub(crate) struct State {
    pub elapsed_time: Duration,
    pub ipc_conn_thread_handle: Option<JoinHandle<()>>,
    pub tx_kill: Option<std::sync::mpsc::Sender<()>>,
    pub rx_baton: Option<std::sync::mpsc::Receiver<IpcThreadMessage>>,
    pub connection_status: Option<ChannelMessage>,
    pub latest_baton_send: Option<String>,
    pub active_baton_connection: bool,
    pub recv: Option<std::sync::mpsc::Receiver<ChannelMessage>>,

    pub card_open: bool, 
    pub altitude_toggle: bool,
    pub airspeed_toggle: bool,
    pub vertical_airspeed_toggle: bool,
    pub other_toggle: bool,

}
