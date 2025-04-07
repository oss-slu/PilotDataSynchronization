#[derive(Debug, Clone, Copy, Default)]
pub enum ChannelMessage {
    Connect,
    #[default]
    Disconnected,
}
