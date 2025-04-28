use std::thread::JoinHandle;

use iced::time::Duration;

use crate::ChannelMessage;
use crate::mychart::MyChart;

#[derive(Default)]
#[allow(unused)]
pub(crate) struct State {
    pub elapsed_time: Duration,
    pub thread_handle: Option<JoinHandle<()>>,
    pub tx_kill: Option<std::sync::mpsc::Sender<()>>,
    pub rx_baton: Option<std::sync::mpsc::Receiver<f32>>,
    pub connection_status: ChannelMessage,
    pub latest_baton_send: Option<f32>,
    pub recv: Option<std::sync::mpsc::Receiver<ChannelMessage>>,
    pub chart: MyChart,
}
