/// Errors raised by entity- and resource-authority operations.
///
/// `Resource*` variants are raised by the Replicated Resources auth API
/// (e.g. `server.resource_take_authority::<R>()`) and are semantically
/// distinct from the entity variants — using `NotInScope` for "the
/// resource isn't currently inserted" would be a misuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityError {
    /// Entity is not configured for delegation.
    NotDelegated,
    /// Authority is not currently `Available` (e.g. another holder).
    NotAvailable,
    /// Caller does not currently hold authority.
    NotHolder,
    /// Entity is not in the user's scope.
    NotInScope,
    /// (Resource API) The resource of the requested type `R` is not
    /// currently inserted on this server/world. Distinct from
    /// `NotInScope` (which is an entity-scope concept). Returned by
    /// `server.resource_take_authority::<R>()`,
    /// `server.resource_release_authority::<R>()`, and the client
    /// `request_resource_authority` / `release_resource_authority`
    /// commands when `R` is missing from the registry.
    ResourceNotPresent,
}
