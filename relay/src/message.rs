#[derive(Debug, Clone, Copy)]
pub(crate) enum Message {
    // generic time update signal
    Update,

    // signal for pre-shutdown procedures and the ID for our window, which is to be closed
    WindowCloseRequest(iced::window::Id),

    // signal to check the baton thread
    BatonMessage,
}

// Enum for messages from within the IPC connection thread
pub(crate) enum IpcThreadMessage {
    // Baton data to be sent over TCP
    BatonData(String),

    // signal that Baton is disconnecting from our server
    BatonShutdown,
}
