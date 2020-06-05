
use crate::{EntityType, NetEntity};

pub struct EntityManifest<T: EntityType> {
    my_entity: T,
}

impl<T: EntityType> EntityManifest<T> {
    pub fn new<S: NetEntity<T>>(some_entity: &S) -> Self {
        EntityManifest {
            my_entity: NetEntity::<T>::to_type(some_entity),
        }
    }

}