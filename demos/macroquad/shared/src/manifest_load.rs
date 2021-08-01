use naia_shared::Manifest;

use crate::{AuthEvent, ExampleActor, ExampleEvent, KeyCommand, PointActor};

pub fn manifest_load() -> Manifest<ExampleEvent, ExampleActor> {
    let mut manifest = Manifest::<ExampleEvent, ExampleActor>::new();

    manifest.register_event(AuthEvent::get_builder());
    manifest.register_pawn(PointActor::get_builder(), KeyCommand::get_builder());

    manifest
}
