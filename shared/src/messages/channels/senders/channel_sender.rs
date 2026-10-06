use naia_serde::BitWriter;
use naia_socket_shared::Instant;

use crate::messages::abandonment::ConnectionRequestNonce;
use crate::messages::channels::senders::request_sender::LocalRequestId;
use crate::messages::request::GlobalRequestId;
use crate::{
    messages::{message_container::MessageContainer, message_kinds::MessageKinds},
    types::MessageIndex,
    LocalEntityAndGlobalEntityConverterMut, LocalResponseId,
};

/// Core send-side trait implemented by every channel sender variant.
pub trait ChannelSender<P>: Send + Sync {
    /// Queues a Message to be transmitted to the remote host into an internal
    /// buffer. Returns `true` if the message was accepted, `false` if the
    /// channel's queue was full and the message was dropped (reliable) or the
    /// oldest entry was evicted to make room (unreliable).
    fn send_message(&mut self, message: P) -> bool;
    /// For reliable channels, will collect any Messages that need to be resent
    fn collect_messages(&mut self, now: &Instant, rtt_millis: f32);
    /// Returns true if there are queued Messages ready to be written
    fn has_messages(&self) -> bool;
    /// Called when it receives acknowledgement that a Message has been received
    fn notify_message_delivered(&mut self, message_index: MessageIndex);
}

/// Extended sender trait for message channels that writes wire bits and supports request/response lifecycle.
pub trait MessageChannelSender: ChannelSender<MessageContainer> {
    /// Gets Messages from the internal buffer and writes it to the `BitWriter`
    fn write_messages(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        writer: &mut BitWriter,
        has_written: &mut bool,
    ) -> Option<Vec<MessageIndex>>;

    /// Queues a Request to be transmitted to the remote host into an internal buffer.
    ///
    /// H3: `nonce` names the exchange on the wire (envelope cutover,
    /// codec grammar 2).
    fn send_outgoing_request(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        global_request_id: GlobalRequestId,
        nonce: ConnectionRequestNonce,
        request: MessageContainer,
    ) -> bool;

    /// Queues a Response to be transmitted to the remote host into an internal buffer.
    ///
    /// H3: `nonce` echoes the incoming request's nonce on the wire.
    fn send_outgoing_response(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        local_response_id: LocalResponseId,
        nonce: ConnectionRequestNonce,
        response: MessageContainer,
    ) -> bool;

    /// Request is finished, so clean up the local request id and return the global request id.
    ///
    /// H3: resolves only when `wire_nonce` names the outstanding exchange.
    fn process_incoming_response(
        &mut self,
        local_request_id: LocalRequestId,
        wire_nonce: ConnectionRequestNonce,
    ) -> Option<GlobalRequestId>;
}
