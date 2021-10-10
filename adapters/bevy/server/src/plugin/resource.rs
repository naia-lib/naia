pub struct ServerResource {
    pub ticked: bool,
}

impl ServerResource {
    pub fn new() -> Self {
        return ServerResource {
            ticked: false,
        }
    }
}