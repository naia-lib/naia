pub struct ClientResource {
    ticked: bool,
}

impl ClientResource {
    pub fn new() -> Self {
        Self { ticked: false }
    }

    // Events //

    // Ticks //

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
