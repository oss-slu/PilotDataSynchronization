use std::thread::JoinHandle;

use iced::time::Duration;

#[derive(Default)]
#[allow(unused)]
pub(crate) struct State {
    pub elapsed_time: Duration,
    pub thread_handle: Option<JoinHandle<()>>,
    pub tx_kill: Option<std::sync::mpsc::Sender<()>>,
    pub rx_baton: Option<std::sync::mpsc::Receiver<f32>>,
    pub latest_baton_send: Option<f32>,
}
