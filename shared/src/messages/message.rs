use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::Mutex,
};

use lazy_static::lazy_static;

use naia_serde::{BitReader, BitWrite, SerdeErr};

use crate::{messages::named::Named, EntityHandle, MessageId, NetEntityHandleConverter};

// Messages
pub struct Messages;

impl Messages {
    pub fn add_message<M: Message + 'static>() {
        let type_id = TypeId::of::<M>();
        let mut messages_data = MESSAGES_DATA.lock().unwrap();
        let message_id = MessageId::new(messages_data.current_id);
        messages_data.type_to_id_map.insert(type_id, message_id);
        messages_data.current_id += 1;
        //TODO: check for current_id overflow?
    }

    pub fn type_to_id<M: Message>() -> MessageId {
        let type_id = TypeId::of::<M>();
        let mut messages_data = MESSAGES_DATA.lock().unwrap();
        return *messages_data.type_to_id_map.get(&type_id).expect(
            "Must properly initialize Message with Protocol via `add_message()` function!",
        );
    }

    pub fn message_id_from_box(boxed_message: &Box<dyn Message>) -> MessageId {
        todo!()
    }

    pub fn downcast<M: Message>(boxed_message: Box<dyn Message>) -> Option<M> {
        let boxed_any: Box<dyn Any> = boxed_message.into_any();
        Box::<dyn Any + 'static>::downcast::<M>(boxed_any)
            .ok()
            .map(|boxed_m| *boxed_m)
    }

    pub fn read(
        bit_reader: &mut BitReader,
        converter: &dyn NetEntityHandleConverter,
    ) -> Result<Box<dyn Message>, SerdeErr> {
        todo!()
    }

    pub fn write(
        bit_writer: &mut dyn BitWrite,
        converter: &dyn NetEntityHandleConverter,
        message: &Box<dyn Message>,
    ) {
        todo!()
    }
}

lazy_static! {
    static ref MESSAGES_DATA: Mutex<MessagesData> = Mutex::new(MessagesData::new());
}

struct MessagesData {
    pub current_id: u16,
    pub type_to_id_map: HashMap<TypeId, MessageId>,
}

impl MessagesData {
    pub fn new() -> Self {
        Self {
            current_id: 0,
            type_to_id_map: HashMap::new(),
        }
    }
}

// Message
pub trait Message: Send + Sync + Named + MessageClone + Any {
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
    fn has_entity_properties(&self) -> bool;
    /// Returns a list of Entities contained within the Replica's properties
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
