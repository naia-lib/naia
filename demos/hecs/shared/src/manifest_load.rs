use naia_shared::Manifest;

use super::{
    components::{Components, Marker, Name, Position},
    messages::{Auth, Events, StringMessage},
};

pub fn manifest_load() -> Manifest<Events, Components> {
    let mut manifest = Manifest::<Events, Components>::new();

    manifest.register_event(Auth::get_builder());
    manifest.register_event(StringMessage::get_builder());

    manifest.register_replica(Position::get_builder());
    manifest.register_replica(Name::get_builder());
    manifest.register_replica(Marker::get_builder());

    manifest
}
