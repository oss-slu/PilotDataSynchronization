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
    RollToggle(bool),
    PitchToggle(bool),
    YawToggle(bool),
    GForceToggle(bool),

    // Messages for the GUI Card pop-up
    CardOpen,
    CardClose,

    ConnectIpc,

    DisconnectIpc,

    ConnectTcp,

    DisconnectTcp,

    TcpAddrFieldUpdate(String),
    SavedTcpAddrSelected(String),

    // Second destination for the same packets, read by the Python side
    MirrorAddrFieldUpdate(String),
    ConnectMirror,
    DisconnectMirror,

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
    // outgoing payload to TCP thread
    Send(String),

    // a fully built packet to write as-is, used by the Send Packet button
    SendRaw(String),

    /// Point the mirror at an address, or `None` to drop it. The mirror gets a
    /// copy of every packet iMotions receives.
    SetMirror(Option<String>),
}

pub(crate) enum FromTcpThreadMessage {
    /// TCP thread successfully connected to remote
    Connected,
    /// TCP thread disconnected (may include reason)
    Disconnected(String),
    /// A packet was sent to remote (In bytes)
    Sent(usize),
    /// Error reported
    SendError(String),
    /// Mirror destination accepted the connection
    MirrorConnected(String),
    /// Mirror was dropped, by the user or by a failed connect or write
    MirrorDisconnected(String),
}
