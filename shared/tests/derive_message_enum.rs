//! `#[derive(Message)]` on enums (naia-lib/naia#163).
//!
//! `derive_enum.rs` covers `#[derive(Serde)]` on enums, which is a different
//! code path: `Serde` writes a bare value, while `Message` writes a kind tag
//! read back through a registered `MessageBuilder`. These tests drive the
//! `Message` path end to end -- register the enum on a `Protocol`, write it
//! through `MessageContainer`, read it back out of `MessageKinds`, and downcast
//! -- for all three variant styles the derive supports.

mod enum_message {
    use naia_shared::Message;

    #[derive(Message, Debug, PartialEq)]
    pub enum TestEnumMessage {
        Ping,
        Chat { text: String },
        Move(u32, u32),
    }
}

use naia_shared::{BitReader, BitWriter, FakeEntityConverter, MessageContainer, Protocol};

use enum_message::TestEnumMessage;

fn protocol() -> Protocol {
    let mut builder = Protocol::builder();
    builder.add_message::<TestEnumMessage>();
    builder.build()
}

/// Writes each message, then reads them all back in order out of one bit stream.
fn round_trip(messages: Vec<TestEnumMessage>) -> Vec<TestEnumMessage> {
    let protocol = protocol();
    let message_kinds = &protocol.message_kinds;

    let mut writer = BitWriter::new();
    let count = messages.len();
    for message in messages {
        let container = MessageContainer::new(Box::new(message));
        container.write(message_kinds, &mut writer, &mut FakeEntityConverter);
    }
    let bytes = writer.to_bytes();

    let mut reader = BitReader::new(&bytes);
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let container = message_kinds
            .read(&mut reader, &FakeEntityConverter)
            .expect("enum message should decode");
        let message = container
            .to_boxed_any()
            .downcast::<TestEnumMessage>()
            .expect("decoded message should be a TestEnumMessage");
        out.push(*message);
    }
    out
}

#[test]
fn a_unit_variant_survives_a_round_trip() {
    assert_eq!(
        round_trip(vec![TestEnumMessage::Ping]),
        vec![TestEnumMessage::Ping],
    );
}

#[test]
fn a_named_field_variant_survives_a_round_trip() {
    let sent = TestEnumMessage::Chat {
        text: "hello enum!".to_string(),
    };
    assert_eq!(
        round_trip(vec![TestEnumMessage::Chat {
            text: "hello enum!".to_string(),
        }]),
        vec![sent],
    );
}

#[test]
fn an_unnamed_field_variant_survives_a_round_trip() {
    assert_eq!(
        round_trip(vec![TestEnumMessage::Move(5851, 42)]),
        vec![TestEnumMessage::Move(5851, 42)],
    );
}

/// The variant discriminant is written as a fixed-width tag, so a stream of
/// mixed variants only decodes correctly if every variant's payload consumes
/// exactly the bits it wrote. Reading them back in one pass proves that.
#[test]
fn mixed_variants_decode_in_order_from_one_stream() {
    let out = round_trip(vec![
        TestEnumMessage::Move(1, 2),
        TestEnumMessage::Ping,
        TestEnumMessage::Chat {
            text: "third".to_string(),
        },
        TestEnumMessage::Ping,
    ]);

    assert_eq!(
        out,
        vec![
            TestEnumMessage::Move(1, 2),
            TestEnumMessage::Ping,
            TestEnumMessage::Chat {
                text: "third".to_string(),
            },
            TestEnumMessage::Ping,
        ],
    );
}
