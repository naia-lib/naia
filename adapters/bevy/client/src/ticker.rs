pub struct Ticker {
    pub ticked: bool,
}

impl Ticker {
    pub fn new() -> Self {
        Self { ticked: false }
    }
}
