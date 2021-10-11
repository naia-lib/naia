use bevy::{ecs::{schedule::ShouldRun, system::{Res, ResMut}}};

use crate::ticker::Ticker;

pub fn should_tick(ticker: Res<Ticker>) -> ShouldRun {
    if ticker.has_ticked() {
        return ShouldRun::Yes;
    } else {
        return ShouldRun::No;
    }
}

pub fn finish_tick(mut ticker: ResMut<Ticker>) {
    ticker.tick_finish();
}