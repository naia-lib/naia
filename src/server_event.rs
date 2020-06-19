
use super::{
    user::{User, UserKey},
};

pub enum ServerEvent<T> {
    Connection(UserKey),
    Disconnection(UserKey, User),
    Event(UserKey, T),
    Tick,
}