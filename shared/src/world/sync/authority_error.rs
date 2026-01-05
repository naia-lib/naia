#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityError {
    NotDelegated,
    NotAvailable,
    NotHolder,
    NotInScope,
}

