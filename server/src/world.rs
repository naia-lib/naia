
/// Structures that implement the WorldType trait will be able to be loaded into the Server
/// at which point the Server will use this interface to keep the WorldType in-sync with it's own Entities/Components
pub trait WorldType {

}

pub struct World {

}

impl World {
    pub fn new() -> Self {
        World {

        }
    }
}

impl WorldType for World {

}