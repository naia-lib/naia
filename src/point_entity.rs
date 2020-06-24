
use std::{
    rc::Rc,
    cell::RefCell,
    io::Cursor,
};

use std::any::{TypeId};

use gaia_shared::{Entity, StateMask, EntityBuilder, EntityMutator, Property, PropertyIo};

use crate::{ExampleEntity};

pub struct PointEntity {
    pub x: Property<u8>, //TODO: Candidate for Macro
    pub y: Property<u8>, //TODO: Candidate for Macro
}

//TODO: Candidate for Macro
#[repr(u8)]
enum PointEntityProp {
    X = 0,
    Y = 1,
}

//TODO: Candidate for Macro
pub struct PointEntityBuilder {
    type_id: TypeId,
}

//TODO: Candidate for Macro
impl EntityBuilder<ExampleEntity> for PointEntityBuilder {
    //TODO: Candidate for Macro
    fn build(&self, buffer: &[u8]) -> ExampleEntity {
        return PointEntity::read_to_type(buffer);
    }

    //TODO: Candidate for Macro
    fn get_type_id(&self) -> TypeId {
        return self.type_id;
    }
}

impl PointEntity {

    //represents a custom constructor method that uses new_complete()
    pub fn new(x: u8, y: u8) -> PointEntity {
        return PointEntity::new_complete(x, y);
    }

    pub fn step(&mut self) {
        let mut x = *self.x.get();
        x += 1;
        if x > 20 {
            x = 0;
        }
        if x % 3 == 0 {
            let mut y = *self.y.get();
            y += 1;
            self.y.set(y);
        }
        self.x.set(x);
    }

    //TODO: Candidate for Macro
    pub fn get_builder() -> Box<dyn EntityBuilder<ExampleEntity>> {
        return Box::new(PointEntityBuilder {
            type_id: TypeId::of::<PointEntity>(),
        });
    }

    //TODO: Candidate for Macro
    pub fn wrap(self) -> Rc<RefCell<PointEntity>> {
        return Rc::new(RefCell::new(self));
    }

    //TODO: Candidate for Macro
    pub fn new_complete(x: u8, y: u8) -> PointEntity {
        PointEntity {
            x: Property::<u8>::new(x, PointEntityProp::X as u8),
            y: Property::<u8>::new(y, PointEntityProp::Y as u8),
        }
    }

    //TODO: Candidate for Macro
    fn read_to_type(buffer: &[u8]) -> ExampleEntity {
        let read_cursor = &mut Cursor::new(buffer);
        let mut x = Property::<u8>::new(Default::default(), PointEntityProp::X as u8);
        x.read(read_cursor);
        let mut y = Property::<u8>::new(Default::default(), PointEntityProp::Y as u8);
        y.read(read_cursor);

        return ExampleEntity::PointEntity(Rc::new(RefCell::new(PointEntity {
            x,
            y,
        })));
    }
}

impl Entity<ExampleEntity> for PointEntity {

    //TODO: Candidate for Macro? Just count properties
    fn get_state_mask_size(&self) -> u8 {
        1
    }

    //TODO: Candidate for Macro (if we refactor out some kind of copy() method
    fn get_typed_copy(&self) -> ExampleEntity {
        let copied_entity = PointEntity::new_complete(*self.x.get(), *self.y.get()).wrap();
        return ExampleEntity::PointEntity(copied_entity);
    }

    //TODO: Candidate for Macro
    fn get_type_id(&self) -> TypeId {
        return TypeId::of::<PointEntity>();
    }

    //TODO: Candidate for Macro
    fn write(&self, buffer: &mut Vec<u8>) {
        PropertyIo::write(&self.x, buffer);
        PropertyIo::write(&self.y, buffer);
    }

    //TODO: Candidate for Macro
    fn write_partial(&self, state_mask: &StateMask, buffer: &mut Vec<u8>) {
        if let Some(true) = state_mask.get_bit(PointEntityProp::X as u8) {
            PropertyIo::write(&self.x, buffer);
        }
        if let Some(true) = state_mask.get_bit(PointEntityProp::Y as u8) {
            PropertyIo::write(&self.y, buffer);
        }
    }

    //TODO: Candidate for Macro
    fn read_partial(&mut self, state_mask: &StateMask, buffer: &[u8]) {
        let read_cursor = &mut Cursor::new(buffer);
        if let Some(true) = state_mask.get_bit(PointEntityProp::X as u8) {
            PropertyIo::read(&mut self.x, read_cursor);
        }
        if let Some(true) = state_mask.get_bit(PointEntityProp::Y as u8) {
            PropertyIo::read(&mut self.y, read_cursor);
        }
    }

    //TODO: Candidate for Macro
    fn set_mutator(&mut self, mutator: &Rc<RefCell<dyn EntityMutator>>) {
        self.x.set_mutator(mutator);
        self.y.set_mutator(mutator);
    }
}