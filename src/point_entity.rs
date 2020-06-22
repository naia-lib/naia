
use std::{
    rc::Rc,
    cell::RefCell,
};

use std::any::{TypeId};

use gaia_shared::{Entity, StateMask, EntityBuilder, EntityMutator};

use crate::{ExampleEntity};

pub struct PointEntity {
    mutator: Option<Rc<RefCell<dyn EntityMutator>>>,
    x: u8,
    y: u8,
}

#[repr(u8)]
enum PointEntityProp {
    X = 0,
    Y = 1,
}

pub struct PointEntityBuilder {
    type_id: TypeId,
}

impl EntityBuilder<ExampleEntity> for PointEntityBuilder {
    fn build(&self, buffer: &[u8]) -> ExampleEntity {
        let entity = PointEntity {
            mutator: None,
            x: buffer[0],
            y: buffer[1],
        };
        return entity.to_type();
    }

    fn get_type_id(&self) -> TypeId {
        return self.type_id;
    }
}

impl PointEntity {

    pub fn get_builder() -> Box<dyn EntityBuilder<ExampleEntity>> {
        return Box::new(PointEntityBuilder {
            type_id: TypeId::of::<PointEntity>(),
        });
    }

    pub fn new(x: u8, y: u8) -> Rc<RefCell<PointEntity>> {
        Rc::new(RefCell::new(PointEntity {
            mutator: None,
            x,
            y,
        }))
    }

    pub fn get_x(&self) -> u8 {
        self.x
    }

    pub fn get_y(&self) -> u8 {
        self.y
    }

    pub fn set_x(&mut self, value: u8) {
        self.x = value;
        self.notify_mutation(PointEntityProp::X);
    }

    pub fn set_y(&mut self, value: u8) {
        self.y = value;
        self.notify_mutation(PointEntityProp::Y);
    }

    pub fn step(&mut self) {
        let mut x = self.get_x();
        x += 1;
        if x > 20 {
            x = 0;
        }
        if x % 3 == 0 {
            let mut y = self.get_y();
            y += 1;
            self.set_y(y);
        }
        self.set_x(x);
    }

    fn notify_mutation(&mut self, prop: PointEntityProp) {
        if let Some(mutator) = &self.mutator {
            mutator.as_ref().borrow_mut().mutate(prop as u8);
        }
    }
}

impl Entity<ExampleEntity> for PointEntity {

    fn get_state_mask_size(&self) -> u8 {
        1
    }

    //to_type COPIES the current entity,
    fn to_type(&self) -> ExampleEntity {
        let copied_entity = Rc::new(RefCell::new(PointEntity {
            mutator: None,
            x: self.get_x(),
            y: self.get_y()
        }));
        return ExampleEntity::PointEntity(copied_entity);
    }

    fn get_type_id(&self) -> TypeId {
        return TypeId::of::<PointEntity>();
    }

    fn write(&self, buffer: &mut Vec<u8>) {
        buffer.push(self.get_x());
        buffer.push(self.get_y());
    }

    fn write_partial(&self, state_mask_ref: &Rc<RefCell<StateMask>>, buffer: &mut Vec<u8>) {
        let state_mask = state_mask_ref.as_ref().borrow();
        if let Some(true) = state_mask.get_bit(PointEntityProp::X as u8) {
            buffer.push(self.get_x());
        }
        if let Some(true) = state_mask.get_bit(PointEntityProp::Y as u8) {
            buffer.push(self.get_y());
        }
    }

    fn read_partial(&mut self, state_mask: &StateMask, buffer: &[u8]) {
        if let Some(true) = state_mask.get_bit(PointEntityProp::X as u8) {
            self.set_x(buffer[0]);
        }
        if let Some(true) = state_mask.get_bit(PointEntityProp::Y as u8) {
            self.set_y(buffer[1]);
        }
    }

    fn print(&self, key: u16) {
        info!("entity print(), key: {}, x: {}, y: {}", key, self.get_x(), self.get_y());
    }

    fn set_mutator(&mut self, mutator: &Rc<RefCell<dyn EntityMutator>>) {
        self.mutator = Some(mutator.clone());
    }
}