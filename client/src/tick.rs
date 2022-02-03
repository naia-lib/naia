#[derive(Debug, Clone, Copy)]
pub struct Tick(pub u16);

impl Tick {
    pub fn u16(&self) -> u16 {
        return self.0;
    }
}
