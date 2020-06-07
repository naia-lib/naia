
use gaia_shared::{EntityManifest};
use crate::{ExampleEntity, PointEntity};

pub fn entity_manifest_load() -> EntityManifest<ExampleEntity> {
    let mut manifest = EntityManifest::<ExampleEntity>::new();

    manifest.register_entity(&PointEntity::init());

    manifest
}
