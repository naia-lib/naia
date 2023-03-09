use naia_derive::MessageInternal;

use crate::{
    messages::channels::{
        receivers::fragment_receiver::FragmentReceiver,
        senders::message_fragmenter::MessageFragmenter,
    },
    FakeEntityConverter, MessageContainer, MessageKinds, Protocol,
};

#[derive(MessageInternal)]
pub struct StringMessage {
    pub inner: String,
}

impl StringMessage {
    pub fn new(inner: &str) -> Self {
        Self {
            inner: inner.to_string(),
        }
    }
}

fn setup() -> (
    MessageKinds,
    FakeEntityConverter,
    MessageFragmenter,
    FragmentReceiver,
) {
    // Protocol
    let mut protocol = Protocol::builder();
    protocol.add_message::<StringMessage>();

    // Converter
    let converter = FakeEntityConverter;

    // Fragmenter
    let fragmenter = MessageFragmenter::new();

    // Fragment Receiver
    let receiver = FragmentReceiver::new();

    (protocol.message_kinds, converter, fragmenter, receiver)
}

#[test]
fn convert_single_fragment() {
    let (message_kinds, converter, mut fragmenter, mut receiver) = setup();

    // Message
    let initial_message = StringMessage::new("hello");
    let outgoing_message = initial_message.clone();

    let container = MessageContainer::from(Box::new(outgoing_message), &FakeEntityConverter);

    // Fragment Message
    let fragments = fragmenter.fragment_message(&message_kinds, &converter, container);
    let fragment_count = fragments.len();

    // Receive Fragments
    let mut incoming_message_container_opt = None;
    for fragment in fragments {
        if let Some((_, reassembled_message)) =
            receiver.receive(&message_kinds, &converter, fragment)
        {
            incoming_message_container_opt = Some(reassembled_message);
            break;
        }
    }
    let Some(incoming_message_container) = incoming_message_container_opt else {
        panic!("Did not receive reassembled message!");
    };
    let Ok(incoming_message) = incoming_message_container.to_boxed_any().downcast::<StringMessage>() else {
        panic!("cannot cast message container into proper message!");
    };

    // Compare
    assert_eq!(fragment_count, 1);
    assert_eq!(initial_message.inner, incoming_message.inner);
}

#[test]
fn convert_multiple_fragments() {
    let (message_kinds, converter, mut fragmenter, mut receiver) = setup();

    // Message
    let initial_message = StringMessage::new("Lorem ipsum dolor sit amet, consectetur adipiscing elit. Donec sed justo a mi ultricies ultrices. \
            Sed porta, odio eu rhoncus venenatis, massa elit posuere nisl, sit amet venenatis mi mi at erat. \
            Vivamus non ullamcorper augue, non pharetra augue. Fusce erat ante, iaculis id maximus eu, faucibus consequat nisl. \
            Integer consequat consequat bibendum. Cras nisl est, lacinia nec ipsum vitae, pulvinar elementum orci. \
            Nullam in mauris vel nulla convallis laoreet vitae in turpis. Proin dignissim mattis ante, a lacinia neque sodales id. \
            Donec ac sollicitudin nunc. Nam luctus nulla ut nisi tristique, quis scelerisque neque elementum. Etiam a quam turpis. \
            Vestibulum ultricies dui et porttitor blandit. Etiam turpis quam, pretium ac convallis a, blandit sit amet ipsum. \
            Sed ut pharetra arcu. Pellentesque id magna sapien. Suspendisse potenti. \
            Donec ut purus venenatis, mollis est ut, sollicitudin egestas.");
    let outgoing_message = initial_message.clone();

    let container = MessageContainer::from(Box::new(outgoing_message), &FakeEntityConverter);

    // Fragment Message
    let fragments = fragmenter.fragment_message(&message_kinds, &converter, container);
    let fragment_count = fragments.len();

    // Receive Fragments
    let mut incoming_message_container_opt = None;
    for i in 0..fragments.len() {
        // shuffle indexes
        let j = match i {
            0 => 1,
            1 => 0,
            x => x,
        };

        let fragment = &fragments[j];
        if let Some((_, reassembled_message)) =
            receiver.receive(&message_kinds, &converter, fragment.clone())
        {
            incoming_message_container_opt = Some(reassembled_message);
            break;
        }
    }
    let Some(incoming_message_container) = incoming_message_container_opt else {
        panic!("Did not receive reassembled message!");
    };
    let Ok(incoming_message) = incoming_message_container.to_boxed_any().downcast::<StringMessage>() else {
        panic!("cannot cast message container into proper message!");
    };

    // Compare
    assert_eq!(fragment_count, 3);
    assert_eq!(initial_message.inner, incoming_message.inner);
}
