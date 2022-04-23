pub use inner::{EmptyEntity, EmptyWorldMut, EmptyWorldRef};

mod inner {
    use std::marker::PhantomData;

    use naia_shared::{
        ComponentUpdate, NetEntityHandleConverter, ProtocolInserter, Protocolize,
        ReplicaDynRefWrapper, ReplicaMutWrapper, ReplicaRefWrapper, Replicate, ReplicateSafe,
        WorldMutType, WorldRefType,
    };

    pub type EmptyEntity = u8;

    // EmptyWorldRef //

    pub struct EmptyWorldRef<P: Protocolize> {
        phantom: PhantomData<P>,
    }

    impl<P: Protocolize> Default for EmptyWorldRef<P> {
        fn default() -> Self {
            Self {
                phantom: PhantomData,
            }
        }
    }

    // EmptyWorldMut //

    pub struct EmptyWorldMut<P: Protocolize> {
        phantom: PhantomData<P>,
    }

    impl<P: Protocolize> Default for EmptyWorldMut<P> {
        fn default() -> Self {
            Self {
                phantom: PhantomData,
            }
        }
    }

    // WorldRefType //

    impl<P: Protocolize> WorldRefType<P, EmptyEntity> for EmptyWorldRef<P> {
        fn has_entity(&self, _entity: &EmptyEntity) -> bool {
            unimplemented!()
        }

        fn entities(&self) -> Vec<EmptyEntity> {
            unimplemented!()
        }

        fn has_component<R: ReplicateSafe<P>>(&self, _entity: &EmptyEntity) -> bool {
            unimplemented!()
        }

        fn has_component_of_kind(&self, _entity: &EmptyEntity, _component_kind: &P::Kind) -> bool {
            unimplemented!()
        }

        fn component<'a, R: ReplicateSafe<P>>(
            &'a self,
            _entity: &EmptyEntity,
        ) -> Option<ReplicaRefWrapper<'a, P, R>> {
            unimplemented!()
        }

        fn component_of_kind<'a>(
            &'a self,
            _entity: &EmptyEntity,
            _component_kind: &P::Kind,
        ) -> Option<ReplicaDynRefWrapper<'a, P>> {
            unimplemented!()
        }
    }

    impl<P: Protocolize> WorldRefType<P, EmptyEntity> for EmptyWorldMut<P> {
        fn has_entity(&self, _entity: &EmptyEntity) -> bool {
            unimplemented!()
        }

        fn entities(&self) -> Vec<EmptyEntity> {
            unimplemented!()
        }

        fn has_component<R: ReplicateSafe<P>>(&self, _entity: &EmptyEntity) -> bool {
            unimplemented!()
        }

        fn has_component_of_kind(&self, _entity: &EmptyEntity, _component_kind: &P::Kind) -> bool {
            unimplemented!()
        }

        fn component<'a, R: ReplicateSafe<P>>(
            &'a self,
            _entity: &EmptyEntity,
        ) -> Option<ReplicaRefWrapper<'a, P, R>> {
            unimplemented!()
        }

        fn component_of_kind<'a>(
            &'a self,
            _entity: &EmptyEntity,
            _component_kind: &P::Kind,
        ) -> Option<ReplicaDynRefWrapper<'a, P>> {
            unimplemented!()
        }
    }

    impl<P: Protocolize> WorldMutType<P, EmptyEntity> for EmptyWorldMut<P> {
        fn spawn_entity(&mut self) -> EmptyEntity {
            unimplemented!()
        }

        fn duplicate_entity(&mut self, _entity: &EmptyEntity) -> EmptyEntity {
            unimplemented!()
        }

        fn duplicate_components(
            &mut self,
            _mutable_entity: &EmptyEntity,
            _immutable_entity: &EmptyEntity,
        ) {
            unimplemented!()
        }

        fn despawn_entity(&mut self, _entity: &EmptyEntity) {
            unimplemented!()
        }

        fn component_kinds(&mut self, _entity: &EmptyEntity) -> Vec<P::Kind> {
            unimplemented!()
        }

        fn component_mut<'a, R: ReplicateSafe<P>>(
            &'a mut self,
            _entity: &EmptyEntity,
        ) -> Option<ReplicaMutWrapper<'a, P, R>> {
            unimplemented!()
        }

        fn component_apply_update(
            &mut self,
            _converter: &dyn NetEntityHandleConverter,
            _entity: &EmptyEntity,
            _component_kind: &P::Kind,
            _update: ComponentUpdate<P::Kind>,
        ) {
            unimplemented!()
        }

        fn mirror_entities(
            &mut self,
            _mutable_entity: &EmptyEntity,
            _immutable_entity: &EmptyEntity,
        ) {
            unimplemented!()
        }

        fn mirror_components(
            &mut self,
            _mutable_entity: &EmptyEntity,
            _immutable_entity: &EmptyEntity,
            _component_kind: &P::Kind,
        ) {
            unimplemented!()
        }

        fn insert_component<R: ReplicateSafe<P>>(
            &mut self,
            _entity: &EmptyEntity,
            _component_ref: R,
        ) {
            unimplemented!()
        }

        fn remove_component<R: Replicate<P>>(&mut self, _entity: &EmptyEntity) -> Option<R> {
            unimplemented!()
        }

        fn remove_component_of_kind(
            &mut self,
            _entity: &EmptyEntity,
            _component_kind: &P::Kind,
        ) -> Option<P> {
            unimplemented!()
        }
    }

    impl<P: Protocolize> ProtocolInserter<P, EmptyEntity> for EmptyWorldMut<P> {
        fn insert<I: ReplicateSafe<P>>(&mut self, _: &EmptyEntity, _: I) {
            unimplemented!()
        }
    }
}
