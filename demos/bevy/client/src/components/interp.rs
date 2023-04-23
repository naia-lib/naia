use bevy::prelude::Component;

#[derive(Component)]
pub struct Interp {
    interp: f32,
    pub interp_x: f32,
    pub interp_y: f32,

    last_x: f32,
    last_y: f32,
    pub next_x: f32,
    pub next_y: f32,
}

impl Interp {
    pub fn new(x: i16, y: i16) -> Self {
        let x = x as f32;
        let y = y as f32;
        Self {
            interp: 0.0,
            interp_x: x,
            interp_y: y,

            last_x: x,
            last_y: y,
            next_x: x,
            next_y: y,
        }
    }

    pub(crate) fn next_position(&mut self, next_x: i16, next_y: i16) {
        self.interp = 0.0;
        self.last_x = self.next_x;
        self.last_y = self.next_y;
        self.interp_x = self.next_x;
        self.interp_y = self.next_y;
        self.next_x = next_x as f32;
        self.next_y = next_y as f32;
    }

    pub(crate) fn interpolate(&mut self, interpolation: f32) {
        if self.interp >= 1.0 || interpolation == 0.0 {
            return;
        }
        if self.interp < interpolation {
            self.interp = interpolation;
            self.interp_x = self.last_x + (self.next_x - self.last_x) * self.interp;
            self.interp_y = self.last_y + (self.next_y - self.last_y) * self.interp;
        }
    }
}
