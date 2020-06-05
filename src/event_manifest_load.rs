
use gaia_shared::{EventManifest};
use crate::{ExampleEvent, StringEvent};

pub fn event_manifest_load() -> EventManifest<ExampleEvent> {
    let mut manifest = EventManifest::<ExampleEvent>::new();

    manifest.register_event(&StringEvent::init());

    manifest
}
