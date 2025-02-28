use std::thread::JoinHandle;

use iced::time::Duration;

#[derive(Default)]
pub(crate) struct State {
    pub elapsed_time: Duration,
    pub flicker: bool,
    pub thread_handle: Option<JoinHandle<()>>,
    pub tx: Option<std::sync::mpsc::Sender<()>>,
}
