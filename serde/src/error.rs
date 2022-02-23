/// The error message when failing to deserialize from the bit stream.
#[derive(Clone)]
pub struct DeErr {}

impl std::fmt::Debug for DeErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Bin deserialize error",)
    }
}

impl std::fmt::Display for DeErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

impl std::error::Error for DeErr {}
