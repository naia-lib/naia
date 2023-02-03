type ProtocolKind = u64;

// Message1
pub struct Message1 {
    data: u16,
}

impl Message1 {
    pub const KIND: ProtocolKind = 1;
}

impl From<Packet> for Message1 {
    fn from(packet: Packet) -> Self {
        Message1 { data: 42 }
    }
}

// Message2
pub struct Message2 {
    data: String,
}

impl Message2 {
    pub const KIND: ProtocolKind = 2;
}

impl From<Packet> for Message2 {
    fn from(packet: Packet) -> Self {
        Message2 {
            data: "yo!".to_string(),
        }
    }
}

// Packet
pub struct Packet;

impl Packet {
    pub fn to<T: From<Packet>>(self) -> T {
        T::from(self)
    }
}

#[test]
fn prototype() {
    let events = server.receive();

    for event in events {
        let some_message: (ProtocolKind, Packet) = (Message1::KIND, Packet);

        match some_message {
            (Message1::KIND, packet) => {
                let message_1 = packet.to::<Message1>();
                // do stuff
            }
            (Message2::KIND, packet) => {
                let message_2: Message2 = packet.into();
                // do stuff
            }
            _ => {}
        }
    }
}
