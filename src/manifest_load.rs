
use gaia_shared::{Manifest};
use crate::{ExampleEvent, StringEvent, ExampleEntity, PointEntity};

pub fn manifest_load() -> Manifest<ExampleEvent, ExampleEntity> {
    let mut manifest = Manifest::<ExampleEvent, ExampleEntity>::new();

    manifest.register_event(&StringEvent::init());

    manifest.register_entity(&PointEntity::init());

    manifest
}
