pub use inner::{EmptyEntity, EmptyWorldMut, EmptyWorldRef};

mod inner {
    use std::marker::PhantomData;

    use naia_shared::{ProtocolInserter, Protocolize, ReplicaMutWrapper, ReplicaRefWrapper, Replicate, ReplicateSafe, WorldMutType, WorldRefType, NetEntityHandleConverter, ComponentUpdate, ReplicaDynRefWrapper};

    pub type EmptyEntity = u8;

    // EmptyWorldRef //

    pub struct EmptyWorldRef<P: Protocolize> {
        phantom: PhantomData<P>,
    }

    impl<P: Protocolize> EmptyWorldRef<P> {
        pub fn new() -> Self {
            Self {
                phantom: PhantomData,
            }
        }
    }

    // EmptyWorldMut //

    pub struct EmptyWorldMut<P: Protocolize> {
        phantom: PhantomData<P>,
    }

    impl<P: Protocolize> EmptyWorldMut<P> {
        pub fn new() -> Self {
            Self {
                phantom: PhantomData,
            }
        }
    }

    // WorldRefType //

    impl<P: Protocolize> WorldRefType<P, EmptyEntity> for EmptyWorldRef<P> {
        fn has_entity(&self, _entity: &EmptyEntity) -> bool {
            todo!()
        }

        fn entities(&self) -> Vec<EmptyEntity> {
            todo!()
        }

        fn has_component<R: ReplicateSafe<P>>(&self, _entity: &EmptyEntity) -> bool {
            todo!()
        }

        fn has_component_of_kind(&self, _entity: &EmptyEntity, _component_kind: &P::Kind) -> bool {
            todo!()
        }

        fn component<'a, R: ReplicateSafe<P>>(&'a self, _entity: &EmptyEntity) -> Option<ReplicaRefWrapper<'a, P, R>> {
            todo!()
        }

        fn component_of_kind<'a>(&'a self, _entity: &EmptyEntity, _component_kind: &P::Kind) -> Option<ReplicaDynRefWrapper<'a, P>> {
            todo!()
        }
    }

    impl<P: Protocolize> WorldRefType<P, EmptyEntity> for EmptyWorldMut<P> {
        fn has_entity(&self, _entity: &EmptyEntity) -> bool {
            todo!()
        }

        fn entities(&self) -> Vec<EmptyEntity> {
            todo!()
        }

        fn has_component<R: ReplicateSafe<P>>(&self, _entity: &EmptyEntity) -> bool {
            todo!()
        }

        fn has_component_of_kind(&self, _entity: &EmptyEntity, _component_kind: &P::Kind) -> bool {
            todo!()
        }

        fn component<'a, R: ReplicateSafe<P>>(&'a self, _entity: &EmptyEntity) -> Option<ReplicaRefWrapper<'a, P, R>> {
            todo!()
        }

        fn component_of_kind<'a>(&'a self, _entity: &EmptyEntity, _component_kind: &P::Kind) -> Option<ReplicaDynRefWrapper<'a, P>> {
            todo!()
        }
    }

    impl<P: Protocolize> WorldMutType<P, EmptyEntity> for EmptyWorldMut<P> {
        fn spawn_entity(&mut self) -> EmptyEntity {
            todo!()
        }

        fn duplicate_entity(&mut self, _entity: &EmptyEntity) -> EmptyEntity {
            todo!()
        }

        fn duplicate_components(&mut self, _mutable_entity: &EmptyEntity, _immutable_entity: &EmptyEntity) {
            todo!()
        }

        fn despawn_entity(&mut self, _entity: &EmptyEntity) {
            todo!()
        }

        fn component_kinds(&mut self, _entity: &EmptyEntity) -> Vec<P::Kind> {
            todo!()
        }

        fn component_mut<'a, R: ReplicateSafe<P>>(&'a mut self, _entity: &EmptyEntity) -> Option<ReplicaMutWrapper<'a, P, R>> {
            todo!()
        }

        fn component_apply_update(&mut self, _converter: &dyn NetEntityHandleConverter, _entity: &EmptyEntity, _component_kind: &P::Kind, _update: ComponentUpdate<P::Kind>) {
            todo!()
        }

        fn mirror_entities(&mut self, _mutable_entity: &EmptyEntity, _immutable_entity: &EmptyEntity) {
            todo!()
        }

        fn mirror_components(&mut self, _mutable_entity: &EmptyEntity, _immutable_entity: &EmptyEntity, _component_kind: &P::Kind) {
            todo!()
        }

        fn insert_component<R: ReplicateSafe<P>>(&mut self, _entity: &EmptyEntity, _component_ref: R) {
            todo!()
        }

        fn remove_component<R: Replicate<P>>(&mut self, _entity: &EmptyEntity) -> Option<R> {
            todo!()
        }

        fn remove_component_of_kind(&mut self, _entity: &EmptyEntity, _component_kind: &P::Kind) -> Option<P> {
            todo!()
        }
    }

    impl<P: Protocolize> ProtocolInserter<P, EmptyEntity> for EmptyWorldMut<P> {
        fn insert<I: ReplicateSafe<P>>(&mut self, _: &EmptyEntity, _: I) {
            unimplemented!()
        }
    }
}
