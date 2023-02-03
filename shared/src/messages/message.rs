use crate::messages::named::Named;

pub trait Message: Send + Sync + MessageClone + Named {

}

pub trait MessageClone {
    fn clone_box(&self) -> Box<dyn Message>;
}

impl<T: 'static + Clone + Message> MessageClone for T {
    fn clone_box(&self) -> Box<dyn Message> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn Message> {
    fn clone(&self) -> Box<dyn Message> {
        MessageClone::clone_box(self.as_ref())
    }
}

impl Named for Box<dyn Message> {
    fn name(&self) -> String {
        Named::name(self.as_ref())
    }
}