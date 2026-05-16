use naia_shared::GlobalEntity;

use crate::{RoomKey, UserKey};

/// Events that drive scope re-evaluation.
/// Each variant encodes exactly the (user, entity) pairs that need attention.
pub(crate) enum ScopeChange {
    /// User was added to a room — check all entities in that room for this user.
    UserEnteredRoom(UserKey, RoomKey),
    /// User was removed from a room — entities in that room may need despawning.
    UserLeftRoom(UserKey, RoomKey),
    /// Entity was added to a room — check all users in that room for this entity.
    EntityEnteredRoom(GlobalEntity, RoomKey),
    /// Explicit include/exclude via UserScope API.
    ScopeToggled(UserKey, GlobalEntity, bool),
}
