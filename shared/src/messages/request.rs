use std::{
    fmt::{self, Debug, Formatter},
    hash::{Hash, Hasher},
    marker::PhantomData,
};

use crate::Message;

/// Marker trait for message types that expect a typed response.
pub trait Request: Message {
    /// The corresponding response type returned by the remote endpoint.
    type Response: Response;
}

/// Marker trait for message types that are sent as a reply to a `Request`.
pub trait Response: Message {}

/// Typed token held by the sender to identify a pending request when its response arrives.
///
/// The identity impls below are written by hand rather than derived: a derived
/// `PartialEq`/`Hash` would bound `S`, and no `Response` is `Eq + Hash`, so the
/// keys could never be compared or used as a map key -- which is what they are
/// for. Only the id participates; `S` is a phantom.
pub struct ResponseSendKey<S: Response> {
    response_id: GlobalResponseId,
    phantom_s: PhantomData<S>,
}

impl<S: Response> ResponseSendKey<S> {
    /// Creates a `ResponseSendKey` tied to the given global response ID.
    pub fn new(id: GlobalResponseId) -> Self {
        Self {
            response_id: id,
            phantom_s: PhantomData,
        }
    }

    /// Returns the global response ID carried by this key.
    pub fn response_id(&self) -> GlobalResponseId {
        self.response_id
    }
}

impl<S: Response> Clone for ResponseSendKey<S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S: Response> Copy for ResponseSendKey<S> {}

impl<S: Response> PartialEq for ResponseSendKey<S> {
    fn eq(&self, other: &Self) -> bool {
        self.response_id == other.response_id
    }
}

impl<S: Response> Eq for ResponseSendKey<S> {}

impl<S: Response> Hash for ResponseSendKey<S> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.response_id.hash(state);
    }
}

impl<S: Response> Debug for ResponseSendKey<S> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ResponseSendKey")
            .field(&self.response_id)
            .finish()
    }
}

/// Typed token held by the receiver to identify which request a response answers.
///
/// Identity impls are hand-written for the same reason as [`ResponseSendKey`].
pub struct ResponseReceiveKey<S: Response> {
    request_id: GlobalRequestId,
    phantom_s: PhantomData<S>,
}

impl<S: Response> ResponseReceiveKey<S> {
    /// Creates a `ResponseReceiveKey` tied to the given global request ID.
    pub fn new(request_id: GlobalRequestId) -> Self {
        Self {
            request_id,
            phantom_s: PhantomData,
        }
    }

    /// Returns the global request ID carried by this key.
    pub fn request_id(&self) -> GlobalRequestId {
        self.request_id
    }
}

impl<S: Response> Clone for ResponseReceiveKey<S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S: Response> Copy for ResponseReceiveKey<S> {}

impl<S: Response> PartialEq for ResponseReceiveKey<S> {
    fn eq(&self, other: &Self) -> bool {
        self.request_id == other.request_id
    }
}

impl<S: Response> Eq for ResponseReceiveKey<S> {}

impl<S: Response> Hash for ResponseReceiveKey<S> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.request_id.hash(state);
    }
}

impl<S: Response> Debug for ResponseReceiveKey<S> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ResponseReceiveKey")
            .field(&self.request_id)
            .finish()
    }
}

/// Globally-unique identifier for an outgoing request, spanning all connections.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct GlobalRequestId {
    id: u64,
}

impl GlobalRequestId {
    /// Creates a `GlobalRequestId` from a raw u64.
    pub fn new(id: u64) -> Self {
        Self { id }
    }
}

/// Globally-unique identifier for a response to a specific request.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct GlobalResponseId {
    id: u64,
}

impl GlobalResponseId {
    /// Creates a `GlobalResponseId` from a raw u64.
    pub fn new(id: u64) -> Self {
        Self { id }
    }
}

#[cfg(test)]
mod request_tests {
    use std::{
        collections::{hash_map::DefaultHasher, HashSet},
        hash::{Hash, Hasher},
    };

    use crate::{Message, Request, Response};

    use super::{GlobalRequestId, GlobalResponseId, ResponseReceiveKey, ResponseSendKey};

    #[derive(Message)]
    struct Question {
        value: u8,
    }

    #[derive(Message)]
    struct Answer {
        value: u8,
    }

    impl Request for Question {
        type Response = Answer;
    }
    impl Response for Answer {}

    #[test]
    fn a_send_key_hands_back_the_response_id_it_was_given() {
        let key: ResponseSendKey<Answer> = ResponseSendKey::new(GlobalResponseId::new(7));

        assert_eq!(key.response_id(), GlobalResponseId::new(7));
    }

    #[test]
    fn a_receive_key_hands_back_the_request_id_it_was_given() {
        let key: ResponseReceiveKey<Answer> = ResponseReceiveKey::new(GlobalRequestId::new(7));

        assert_eq!(key.request_id(), GlobalRequestId::new(7));
    }

    #[test]
    fn keys_for_different_exchanges_are_distinct_and_hash_apart() {
        let first: ResponseSendKey<Answer> = ResponseSendKey::new(GlobalResponseId::new(1));
        let second: ResponseSendKey<Answer> = ResponseSendKey::new(GlobalResponseId::new(2));
        let same_as_first: ResponseSendKey<Answer> = ResponseSendKey::new(GlobalResponseId::new(1));

        assert_eq!(first, same_as_first);
        assert_ne!(first, second);

        // The keys index the pending-request tables, so equal keys must
        // collapse to one entry and unequal keys must not.
        let set: HashSet<ResponseSendKey<Answer>> =
            [first, second, same_as_first].into_iter().collect();
        assert_eq!(set.len(), 2);
    }

    fn hash_of<T: Hash>(value: &T) -> u64 {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn receive_keys_are_distinct_and_hash_apart_too() {
        let first: ResponseReceiveKey<Answer> = ResponseReceiveKey::new(GlobalRequestId::new(1));
        let second: ResponseReceiveKey<Answer> = ResponseReceiveKey::new(GlobalRequestId::new(2));
        let same_as_first: ResponseReceiveKey<Answer> =
            ResponseReceiveKey::new(GlobalRequestId::new(1));

        assert_eq!(first, same_as_first);
        assert_ne!(first, second);

        let set: HashSet<ResponseReceiveKey<Answer>> =
            [first, second, same_as_first].into_iter().collect();
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn a_key_hashes_by_its_id_rather_than_landing_every_request_in_one_bucket() {
        let send_one: ResponseSendKey<Answer> = ResponseSendKey::new(GlobalResponseId::new(1));
        let send_two: ResponseSendKey<Answer> = ResponseSendKey::new(GlobalResponseId::new(2));
        let receive_one: ResponseReceiveKey<Answer> =
            ResponseReceiveKey::new(GlobalRequestId::new(1));
        let receive_two: ResponseReceiveKey<Answer> =
            ResponseReceiveKey::new(GlobalRequestId::new(2));

        assert_eq!(
            hash_of(&send_one),
            hash_of(&ResponseSendKey::<Answer>::new(GlobalResponseId::new(1)))
        );
        assert_ne!(hash_of(&send_one), hash_of(&send_two));
        assert_ne!(hash_of(&receive_one), hash_of(&receive_two));

        // The id is what is hashed -- not some constant that would pile every
        // pending request into a single bucket.
        assert_eq!(hash_of(&send_one), hash_of(&GlobalResponseId::new(1)));
        assert_eq!(hash_of(&receive_one), hash_of(&GlobalRequestId::new(1)));
    }

    #[test]
    fn a_key_debug_prints_the_id_it_carries() {
        let send: ResponseSendKey<Answer> = ResponseSendKey::new(GlobalResponseId::new(5));
        let receive: ResponseReceiveKey<Answer> = ResponseReceiveKey::new(GlobalRequestId::new(6));

        assert_eq!(
            format!("{:?}", send),
            "ResponseSendKey(GlobalResponseId { id: 5 })".to_string()
        );
        assert_eq!(
            format!("{:?}", receive),
            "ResponseReceiveKey(GlobalRequestId { id: 6 })".to_string()
        );
    }

    #[test]
    fn a_receive_key_survives_being_copied_around() {
        let key: ResponseReceiveKey<Answer> = ResponseReceiveKey::new(GlobalRequestId::new(3));
        let copied = key;

        assert_eq!(copied.request_id(), key.request_id());
        assert_eq!(copied, key);
    }

    #[test]
    fn the_two_id_types_carry_their_raw_values() {
        assert_eq!(GlobalRequestId::new(1), GlobalRequestId::new(1));
        assert_ne!(GlobalRequestId::new(1), GlobalRequestId::new(2));
        assert_eq!(GlobalResponseId::new(1), GlobalResponseId::new(1));
        assert_ne!(GlobalResponseId::new(1), GlobalResponseId::new(2));
    }

    #[test]
    fn a_request_names_its_own_response_type() {
        fn response_name<Q: Request>() -> String {
            <Q::Response as crate::Named>::protocol_name().to_string()
        }

        assert_eq!(response_name::<Question>(), "Answer".to_string());
    }
}
