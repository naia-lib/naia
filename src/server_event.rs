
use super::{
    user::UserKey,
};

pub enum ServerEvent<T> {
    Connection(UserKey),
    Disconnection(UserKey),
    Event(UserKey, T),
    Tick,
}