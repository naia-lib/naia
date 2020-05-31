use crate::NetBase;

pub struct NetType<T: NetBase> {
    gaia_id: Option<u32>,
    identity: Box<T>,
}

impl<T: NetBase> NetType<T> {
    pub fn init() -> Box<Self> {
        Box::new(
            NetType {
                gaia_id: None,
                identity: T::identity(),
            }
        )
    }
}

pub trait NetTypeTrait {
    fn set_gaia_id(&mut self, gaia_id: u32);
    fn get_gaia_id(&self) -> u32;
}

impl<T: NetBase> NetTypeTrait for NetType<T> {
    fn set_gaia_id(&mut self, gaia_id: u32) {
        self.gaia_id = Some(gaia_id);
    }

    fn get_gaia_id(&self) -> u32 {
        return self.gaia_id.expect("NetType not initialized (& set_gaia_id() function called)");
    }
}