#[derive(Default)]
pub struct Flag {
    set: bool,
}

impl Flag {
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
