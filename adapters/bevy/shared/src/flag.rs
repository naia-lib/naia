pub struct Flag {
    set: bool,
}

impl Flag {
    pub fn new() -> Self {
        Self { set: false }
    }

    pub fn set(&mut self) {
        self.set = true;
    }

    pub fn reset(&mut self) {
        self.set = false;
    }

    pub fn is_set(&self) -> bool {
        self.set
    }
}
