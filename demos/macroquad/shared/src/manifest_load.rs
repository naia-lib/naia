use naia_shared::Manifest;

use super::{events::{Auth, Events, KeyCommand}, objects::{Objects, Point}};

pub fn manifest_load() -> Manifest<Events, Objects> {
    let mut manifest = Manifest::<Events, Objects>::new();

    manifest.register_event(Auth::get_builder());
    manifest.register_pawn(Point::get_builder(), KeyCommand::get_builder());

    manifest
}
