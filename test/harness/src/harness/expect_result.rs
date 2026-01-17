//! ExpectResult type for single-tick expectation outcomes.
//!
//! This enum represents the three possible outcomes of evaluating
//! a Then step predicate on a single tick:
//! - `Passed`: The expectation is satisfied
//! - `NotYet`: Not yet satisfied, runner should retry next tick
//! - `Failed`: Hard failure, do not retry

/// Result of a single-tick expectation check.
///
/// Used by the runner to determine whether to continue polling
/// a Then step or to complete/fail the step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpectResult<T = ()> {
    /// Expectation satisfied with the given value.
    Passed(T),
    /// Not yet satisfied, runner should retry next tick.
    NotYet,
    /// Hard failure with message, do not retry.
    Failed(String),
}

impl<T> ExpectResult<T> {
    /// Returns `true` if the result is `Passed`.
    #[inline]
    pub fn is_passed(&self) -> bool {
        matches!(self, Self::Passed(_))
    }

    /// Returns `true` if the result is `NotYet`.
    #[inline]
    pub fn is_not_yet(&self) -> bool {
        matches!(self, Self::NotYet)
    }

    /// Returns `true` if the result is `Failed`.
    #[inline]
    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }

    /// Converts from `ExpectResult<T>` to `Option<T>`.
    ///
    /// Returns `Some(value)` if `Passed`, otherwise `None`.
    #[inline]
    pub fn ok(self) -> Option<T> {
        match self {
            Self::Passed(v) => Some(v),
            _ => None,
        }
    }

    /// Maps an `ExpectResult<T>` to `ExpectResult<U>` by applying a function
    /// to the contained `Passed` value.
    #[inline]
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> ExpectResult<U> {
        match self {
            Self::Passed(v) => ExpectResult::Passed(f(v)),
            Self::NotYet => ExpectResult::NotYet,
            Self::Failed(msg) => ExpectResult::Failed(msg),
        }
    }
}

impl ExpectResult<()> {
    /// Convenience constructor for a passed unit result.
    #[inline]
    pub fn passed() -> Self {
        Self::Passed(())
    }
}

impl<T> From<Option<T>> for ExpectResult<T> {
    /// Converts `Some(v)` to `Passed(v)`, `None` to `NotYet`.
    fn from(opt: Option<T>) -> Self {
        match opt {
            Some(v) => Self::Passed(v),
            None => Self::NotYet,
        }
    }
}
