
pub trait NetEvent {
    fn read(&self);

    fn write(&self);
}