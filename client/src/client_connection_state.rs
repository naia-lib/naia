#[derive(PartialEq)]
pub enum ClientConnectionState {
    AwaitingChallengeResponse,
    AwaitingConnectResponse,
    Connected,
}
