
use gaia_shared::{Manifest};
use crate::{ExampleType, StringEvent};

pub fn manifest_load() -> Manifest<ExampleType> {
    let mut manifest = Manifest::<ExampleType>::new();

    manifest.register(StringEvent::init());

    manifest
}
