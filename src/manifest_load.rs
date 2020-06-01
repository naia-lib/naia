
use gaia_shared::{NetType, Manifest};

use crate::ExampleEvent;

pub fn manifest_load() -> Manifest {
    let mut manifest = Manifest::new();

    manifest.register_type(ExampleEvent::init());

    manifest
}
