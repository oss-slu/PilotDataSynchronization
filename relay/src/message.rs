#[derive(Debug, Clone)]
pub(crate) enum Message {
    // generic time update signal
    Update,

    // signal for pre-shutdown procedures and the ID for our window, which is to be closed
    WindowCloseRequest(iced::window::Id),

    // signal to check the baton thread
    BatonMessage,

    ConnectionMessage,

    Tick,
}
