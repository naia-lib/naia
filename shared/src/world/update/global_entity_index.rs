/// Dense global index for a server-replicated entity.
///
/// 0 is reserved as the invalid sentinel (`INVALID`). Valid indices start at 1.
/// Shared across all connections — the same entity has the same index for every user.
/// Never appears on the wire; purely an in-memory shortcut for O(1) array access
/// instead of HashMap probe.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct GlobalEntityIndex(pub u32);

impl GlobalEntityIndex {
    /// The invalid sentinel value (index 0). Valid indices start at 1.
    pub const INVALID: Self = Self(0);

    /// Returns `true` if this index is not the `INVALID` sentinel.
    pub fn is_valid(self) -> bool {
        self.0 != 0
    }

    /// Converts this index to a `usize` for use as an array slot.
    pub fn as_usize(self) -> usize {
        self.0 as usize
    }
}

impl From<u32> for GlobalEntityIndex {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<GlobalEntityIndex> for u32 {
    fn from(value: GlobalEntityIndex) -> Self {
        value.0
    }
}
