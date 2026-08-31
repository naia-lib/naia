use crate::{
    ComponentKind, EntityAuthStatus, EntityMessageType, GlobalEntity, RemoteEntity, Replicate, Tick,
};

/// ECS-level event produced by the replication system when the remote world state changes.
pub enum EntityEvent {
    /// A new entity was spawned by the remote.
    Spawn(GlobalEntity),
    /// An existing entity was despawned by the remote.
    Despawn(GlobalEntity),
    /// A component was added to an entity.
    InsertComponent(GlobalEntity, ComponentKind),
    /// A component was removed from an entity; carries the last known component value.
    RemoveComponent(GlobalEntity, Box<dyn Replicate>),
    /// A component on an entity was updated at the given tick.
    UpdateComponent(Tick, GlobalEntity, ComponentKind),

    /// Entity was published (made visible to other users).
    Publish(GlobalEntity),
    /// Entity publication was retracted.
    Unpublish(GlobalEntity),
    /// Authority delegation was enabled for an entity.
    EnableDelegation(GlobalEntity),
    /// Authority delegation was disabled for an entity.
    DisableDelegation(GlobalEntity),
    /// Authority status for a delegated entity was updated.
    SetAuthority(GlobalEntity, EntityAuthStatus),

    /// Client is requesting authority over an entity.
    RequestAuthority(GlobalEntity),
    /// Client is releasing authority over an entity.
    ReleaseAuthority(GlobalEntity),
    /// Client acknowledged that delegation is now enabled.
    EnableDelegationResponse(GlobalEntity),
    /// An entity migrated; carries the new remote entity ID.
    MigrateResponse(GlobalEntity, RemoteEntity),
}

impl EntityEvent {
    /// Returns the [`EntityMessageType`] discriminant for this event, or `None` for `UpdateComponent` (which has no wire type).
    pub fn to_type(&self) -> Option<EntityMessageType> {
        match self {
            Self::Spawn(_) => Some(EntityMessageType::Spawn),
            Self::Despawn(_) => Some(EntityMessageType::Despawn),
            Self::InsertComponent(_, _) => Some(EntityMessageType::InsertComponent),
            Self::RemoveComponent(_, _) => Some(EntityMessageType::RemoveComponent),
            Self::Publish(_) => Some(EntityMessageType::Publish),
            Self::Unpublish(_) => Some(EntityMessageType::Unpublish),
            Self::EnableDelegation(_) => Some(EntityMessageType::EnableDelegation),
            Self::EnableDelegationResponse(_) => Some(EntityMessageType::EnableDelegationResponse),
            Self::DisableDelegation(_) => Some(EntityMessageType::DisableDelegation),
            Self::RequestAuthority(_) => Some(EntityMessageType::RequestAuthority),
            Self::ReleaseAuthority(_) => Some(EntityMessageType::ReleaseAuthority),
            Self::SetAuthority(_, _) => Some(EntityMessageType::SetAuthority),
            Self::MigrateResponse(_, _) => Some(EntityMessageType::MigrateResponse),
            Self::UpdateComponent(_, _, _) => None, // UpdateComponent is not a message type
        }
    }

    /// Returns the [`GlobalEntity`] this event refers to.
    pub fn entity(&self) -> GlobalEntity {
        match self {
            Self::Spawn(entity) => *entity,
            Self::Despawn(entity) => *entity,
            Self::InsertComponent(entity, _) => *entity,
            Self::RemoveComponent(entity, _) => *entity,
            Self::UpdateComponent(_, entity, _) => *entity,
            Self::Publish(entity) => *entity,
            Self::Unpublish(entity) => *entity,
            Self::EnableDelegation(entity) => *entity,
            Self::EnableDelegationResponse(entity) => *entity,
            Self::DisableDelegation(entity) => *entity,
            Self::RequestAuthority(entity) => *entity,
            Self::ReleaseAuthority(entity) => *entity,
            Self::SetAuthority(entity, _) => *entity,
            Self::MigrateResponse(entity, _) => *entity,
        }
    }

    /// Returns a human-readable string describing this event, suitable for debug logging.
    pub fn log(&self) -> String {
        let entity = self.entity();
        if let Some(ev_type) = self.to_type() {
            format!("{:?} {:?}", ev_type, entity)
        } else {
            format!("UpdateComponent {:?}", entity)
        }
    }
}

#[cfg(test)]
mod entity_event_tests {
    use crate::{
        BigMapKey, ComponentKind, EntityAuthStatus, EntityMessageType, GlobalEntity, Property,
        RemoteEntity, Replicate,
    };

    use super::EntityEvent;

    #[derive(Replicate)]
    struct Ghost {
        value: Property<u8>,
    }

    fn entity(id: u64) -> GlobalEntity {
        GlobalEntity::from_u64(id)
    }

    /// One of every variant, each carrying a distinct entity so a mis-wired
    /// arm reading the wrong field would surface as the wrong id.
    fn one_of_each() -> Vec<(EntityEvent, Option<EntityMessageType>, u64)> {
        vec![
            (
                EntityEvent::Spawn(entity(1)),
                Some(EntityMessageType::Spawn),
                1,
            ),
            (
                EntityEvent::Despawn(entity(2)),
                Some(EntityMessageType::Despawn),
                2,
            ),
            (
                EntityEvent::InsertComponent(entity(3), ComponentKind::of::<Ghost>()),
                Some(EntityMessageType::InsertComponent),
                3,
            ),
            (
                EntityEvent::RemoveComponent(entity(4), Box::new(Ghost::new_complete(7))),
                Some(EntityMessageType::RemoveComponent),
                4,
            ),
            (
                EntityEvent::UpdateComponent(9, entity(5), ComponentKind::of::<Ghost>()),
                None,
                5,
            ),
            (
                EntityEvent::Publish(entity(6)),
                Some(EntityMessageType::Publish),
                6,
            ),
            (
                EntityEvent::Unpublish(entity(7)),
                Some(EntityMessageType::Unpublish),
                7,
            ),
            (
                EntityEvent::EnableDelegation(entity(8)),
                Some(EntityMessageType::EnableDelegation),
                8,
            ),
            (
                EntityEvent::DisableDelegation(entity(9)),
                Some(EntityMessageType::DisableDelegation),
                9,
            ),
            (
                EntityEvent::SetAuthority(entity(10), EntityAuthStatus::Granted),
                Some(EntityMessageType::SetAuthority),
                10,
            ),
            (
                EntityEvent::RequestAuthority(entity(11)),
                Some(EntityMessageType::RequestAuthority),
                11,
            ),
            (
                EntityEvent::ReleaseAuthority(entity(12)),
                Some(EntityMessageType::ReleaseAuthority),
                12,
            ),
            (
                EntityEvent::EnableDelegationResponse(entity(13)),
                Some(EntityMessageType::EnableDelegationResponse),
                13,
            ),
            (
                EntityEvent::MigrateResponse(entity(14), RemoteEntity::new(99)),
                Some(EntityMessageType::MigrateResponse),
                14,
            ),
        ]
    }

    #[test]
    fn every_variant_reports_its_own_message_type() {
        for (event, expected, _) in one_of_each() {
            assert_eq!(event.to_type(), expected, "wrong type for {}", event.log());
        }
    }

    #[test]
    fn every_variant_reports_the_entity_it_carries() {
        for (event, _, expected) in one_of_each() {
            assert_eq!(
                BigMapKey::to_u64(&event.entity()),
                expected,
                "wrong entity for {}",
                event.log()
            );
        }
    }

    #[test]
    fn an_update_is_the_only_variant_without_a_wire_type() {
        let without_type: Vec<String> = one_of_each()
            .into_iter()
            .filter(|(event, _, _)| event.to_type().is_none())
            .map(|(event, _, _)| event.log())
            .collect();

        assert_eq!(without_type, vec!["UpdateComponent GlobalEntity(5)"]);
    }

    #[test]
    fn the_log_line_names_the_message_type_and_the_entity() {
        assert_eq!(
            EntityEvent::Spawn(entity(1)).log(),
            "Spawn GlobalEntity(1)".to_string()
        );
        assert_eq!(
            EntityEvent::SetAuthority(entity(2), EntityAuthStatus::Granted).log(),
            "SetAuthority GlobalEntity(2)".to_string()
        );
    }
}
