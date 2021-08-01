use naia_shared::Manifest;

use super::{events::{Auth, Events, KeyCommand}, state::{State, Point}};

pub fn manifest_load() -> Manifest<Events, State> {
    let mut manifest = Manifest::<Events, State>::new();

    manifest.register_event(Auth::get_builder());
    manifest.register_pawn(Point::get_builder(), KeyCommand::get_builder());

    manifest
}
