pub struct Interp {
    next_x: f32,
    next_y: f32,
    last_x: f32,
    last_y: f32,
    interp: f32,
    pub interp_x: f32,
    pub interp_y: f32,
}

impl Interp {
    pub fn new() -> Self {
        Self {
            next_x: 0.0,
            next_y: 0.0,
            last_x: 0.0,
            last_y: 0.0,
            interp: 0.0,
            interp_x: 0.0,
            interp_y: 0.0,
        }
    }

    pub(crate) fn update_position(&mut self, next_x: i16, next_y: i16) {
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
        } else {
            self.interp = 1.0;
            self.interp_x = self.next_x;
            self.interp_y = self.next_y;
        }
    }
}
