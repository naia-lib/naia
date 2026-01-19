/// Trait for types with a stable protocol name.
///
/// Used for protocol identification (hashing) and debugging.
/// The name should be a stable string literal, not dependent on
/// `std::any::type_name` which may vary across compiler versions.
pub trait Named {
    /// Gets the String representation of the type.
    /// Used for debugging and logging with trait objects.
    fn name(&self) -> String;

    /// Gets the stable protocol name for this type.
    /// Used for protocol ID hashing. Only available on sized types.
    fn protocol_name() -> &'static str
    where
        Self: Sized;
}
