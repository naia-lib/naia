pub(crate) mod main_user;
#[expect(
    clippy::module_inception,
    reason = "the module keeps the public API name used by existing callers"
)]
pub(crate) mod user;
pub(crate) mod world_user;

pub use main_user::*;
pub use user::*;
pub use world_user::*;
