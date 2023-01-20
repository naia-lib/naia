/// The error message when failing to deserialize from the bit stream.
#[derive(Clone)]
pub struct SerdeErr;

impl std::fmt::Debug for SerdeErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Deserialize error",)
    }
}

impl std::fmt::Display for SerdeErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

impl std::error::Error for SerdeErr {}

/// The error message when overflowing the write buffer.
#[derive(Clone)]
pub struct WriteOverflowError;

impl std::fmt::Debug for WriteOverflowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Serialize overflow error",)
    }
}

impl std::fmt::Display for WriteOverflowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

impl std::error::Error for WriteOverflowError {}