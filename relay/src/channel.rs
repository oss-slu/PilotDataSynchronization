#[derive(Debug, Clone, Default)]
pub enum ToTcpThreadMessage {
    Connect,
    #[default]
    Disconnect,
    Send(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FromTcpThreadMessage {
    SuccessfullyConnected,
}

// Enum for messages from within the IPC connection thread
pub(crate) enum IpcThreadMessage {
    // Baton data to be sent over TCP
    BatonData(String),

    // signal that Baton is disconnecting from our server
    BatonShutdown,
}
