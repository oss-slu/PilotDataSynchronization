#[derive(Debug, Clone)]
pub(crate) enum Message {
    // generic time update signal
    Update,

    // signal for pre-shutdown procedures and the ID for our window, which is to be closed
    WindowCloseRequest(iced::window::Id),

    ConnectionMessage,

    CreateXMLFile,

    // Toggle buttons to generate XML file in UI
    AltitudeToggle(bool),
    AirspeedToggle(bool),
    VerticalAirspeedToggle(bool),
    HeadingToggle(bool),

    // Messages for the GUI Card pop-up
    CardOpen,
    CardClose,

    ConnectIpc,

    DisconnectIpc,

    ConnectTcp,

    DisconnectTcp,

    TcpAddrFieldUpdate(String),

    SendPacket,
}

pub(crate) enum ToIpcThreadMessage {}

// Enum for messages from within the IPC connection thread
pub(crate) enum FromIpcThreadMessage {
    // Baton data to be sent over TCP
    BatonData(String),

    // signal that Baton is disconnecting from our server
    BatonShutdown,
}

pub(crate) enum ToTcpThreadMessage {
    Send(String),
}

pub(crate) enum FromTcpThreadMessage {}
