#[derive(Debug, PartialEq)]
pub enum LocalityStatus {
    Waiting,
    Creating,
    Created,
    Deleting,
}