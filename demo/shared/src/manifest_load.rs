use crate::{AuthEvent, ExampleActor, ExampleEvent, PointActor, StringEvent};
use naia_shared::Manifest;

pub fn manifest_load() -> Manifest<ExampleEvent, ExampleActor> {
    let mut manifest = Manifest::<ExampleEvent, ExampleActor>::new();

    manifest.register_event(AuthEvent::get_builder());
    manifest.register_event(StringEvent::get_builder());
    manifest.register_actor(PointActor::get_builder());

    manifest
}
