use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::{Mutex, MutexGuard},
};

use lazy_static::lazy_static;

use naia_serde::{BitReader, BitWrite, Serde, SerdeErr};

use crate::{messages::named::Named, EntityHandle, MessageId, NetEntityHandleConverter};

// Messages
pub struct Messages;

impl Messages {
    pub fn add_message<M: Message + 'static>() {
        let type_id = TypeId::of::<M>();
        let builder = M::create_builder();
        Self::get_data().add_message(&type_id, builder);
    }

    pub fn type_to_id<M: Message>() -> MessageId {
        let type_id = TypeId::of::<M>();
        return Self::get_data().get_id(&type_id);
    }

    pub fn read(
        reader: &mut BitReader,
        converter: &dyn NetEntityHandleConverter,
    ) -> Result<Box<dyn Message>, SerdeErr> {
        let component_id: MessageId = MessageId::de(reader)?;
        return Self::get_data()
            .get_builder(&component_id)
            .read(reader, converter);
    }

    pub fn cast<M: Message>(boxed_message: Box<dyn Message>) -> Option<M> {
        let boxed_any: Box<dyn Any> = boxed_message.to_boxed_any();
        Box::<dyn Any + 'static>::downcast::<M>(boxed_any)
            .ok()
            .map(|boxed_m| *boxed_m)
    }

    fn get_data() -> MutexGuard<'static, MessagesData> {
        match MESSAGES_DATA.lock() {
            Ok(messages_data) => {
                return messages_data;
            }
            Err(poison) => {
                panic!("Messages::get_data() Error: {}", poison);
            }
        }
    }
}

lazy_static! {
    static ref MESSAGES_DATA: Mutex<MessagesData> = Mutex::new(MessagesData::new());
}

struct MessagesData {
    current_id: u16,
    type_to_id_map: HashMap<TypeId, MessageId>,
    id_to_data_map: HashMap<MessageId, Box<dyn MessageBuilder>>,
}

impl MessagesData {
    fn new() -> Self {
        Self {
            current_id: 0,
            type_to_id_map: HashMap::new(),
            id_to_data_map: HashMap::new(),
        }
    }

    fn add_message(&mut self, type_id: &TypeId, builder: Box<dyn MessageBuilder>) {
        let message_id = MessageId::new(self.current_id);
        self.type_to_id_map.insert(*type_id, message_id);
        self.id_to_data_map.insert(message_id, builder);
        self.current_id += 1;
        //TODO: check for current_id overflow?
    }

    fn get_id(&self, type_id: &TypeId) -> MessageId {
        return *self.type_to_id_map.get(type_id).expect(
            "Must properly initialize Message with Protocol via `add_message()` function!",
        );
    }

    fn get_builder(&self, id: &MessageId) -> &Box<dyn MessageBuilder> {
        return self.id_to_data_map.get(&id).expect(
            "Must properly initialize Message with Protocol via `add_message()` function!",
        );
    }
}

// MessageBuilder
pub trait MessageBuilder: Send {
    /// Create new Message from incoming bit stream
    fn read(
        &self,
        reader: &mut BitReader,
        converter: &dyn NetEntityHandleConverter,
    ) -> Result<Box<dyn Message>, SerdeErr>;
}

// Message
pub trait Message: Send + Sync + Named + MessageClone + Any {
    fn to_boxed_any(self: Box<Self>) -> Box<dyn Any>;
    fn create_builder() -> Box<dyn MessageBuilder>
    where
        Self: Sized;
    /// Gets the MessageId of this type
    fn kind(&self) -> MessageId;
    /// Writes data into an outgoing byte stream, sufficient to completely
    /// recreate the Component on the client
    fn write(&self, bit_writer: &mut dyn BitWrite, converter: &dyn NetEntityHandleConverter);
    /// Returns whether has any EntityProperties
    fn has_entity_properties(&self) -> bool;
    /// Returns a list of Entities contained within the Message's EntityProperty fields
    fn entities(&self) -> Vec<EntityHandle>;
}

// Named
impl Named for Box<dyn Message> {
    fn name(&self) -> String {
        self.as_ref().name()
    }
}

// MessageClone
pub trait MessageClone {
    fn clone_box(&self) -> Box<dyn Message>;
}

impl<T: 'static + Clone + Message> MessageClone for T {
    fn clone_box(&self) -> Box<dyn Message> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn Message> {
    fn clone(&self) -> Box<dyn Message> {
        MessageClone::clone_box(self.as_ref())
    }
}
