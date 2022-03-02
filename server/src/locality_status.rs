#[derive(Debug, PartialEq, Clone, Copy)]
pub enum LocalityStatus {
    Creating,
    Created,
    Deleting,
}
