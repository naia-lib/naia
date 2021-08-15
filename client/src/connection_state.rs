#[derive(Debug, PartialEq)]
pub enum ConnectionState {
    AwaitingChallengeResponse,
    AwaitingConnectResponse,
    Connected,
}
