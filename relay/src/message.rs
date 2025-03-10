#[derive(Debug)]
pub(crate) enum Message {
    // generic time update signal
    Update,

    // for testing subscription batching, temporary
    Flicker,

    // signal for pre-shutdown procedures and the ID for our window, which is to be closed
    WindowCloseRequest(iced::window::Id),
}
