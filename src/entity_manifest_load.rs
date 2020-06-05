
use gaia_shared::{EntityManifest};
use crate::{ExampleEntity, PointEntity};

pub fn entity_manifest_load() -> EntityManifest<ExampleEntity> {
    let mut manifest = EntityManifest::<ExampleEntity>::new(&PointEntity::init());

    //manifest.register(&PointEntity::init());

    manifest
}
