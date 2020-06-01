use crate::NetBase;

pub struct NetType<T: NetBase> {
    gaia_id: Option<u16>,
    identity: Box<T>,
}

impl<T: NetBase> NetType<T> {
    pub fn init(identity: T) -> Box<Self> {
        Box::new(
            NetType {
                gaia_id: None,
                identity: Box::new(identity)
            }
        )
    }
}

pub trait NetTypeTrait {
    fn set_gaia_id(&mut self, gaia_id: u16);
    fn get_gaia_id(&self) -> u16;
}

impl<T: NetBase> NetTypeTrait for NetType<T> {
    fn set_gaia_id(&mut self, gaia_id: u16) {
        self.gaia_id = Some(gaia_id);
    }

    fn get_gaia_id(&self) -> u16 {
        return self.gaia_id.expect("NetType not initialized (& set_gaia_id() function called)");
    }
}