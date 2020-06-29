#[derive(Debug, PartialEq)]
pub enum ClientConnectionState {
    AwaitingChallengeResponse,
    AwaitingConnectResponse,
    Connected,
}
