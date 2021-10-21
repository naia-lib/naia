use bevy::ecs::{
    schedule::ShouldRun,
    system::{Res, ResMut},
};

pub struct Ticker {
    ticked: bool,
}

impl Ticker {
    pub fn new() -> Self {
        Self { ticked: false }
    }

    pub fn tick_start(&mut self) {
        self.ticked = true;
    }

    pub fn tick_finish(&mut self) {
        self.ticked = false;
    }

    pub fn has_ticked(&self) -> bool {
        return self.ticked;
    }
}

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
