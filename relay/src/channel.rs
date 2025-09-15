#[derive(Debug, Clone, Default)]
pub enum ChannelMessage {
    Connect,
    #[default]
    Disconnected,
}
