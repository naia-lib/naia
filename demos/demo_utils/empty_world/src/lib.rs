pub use inner::{EmptyEntity, EmptyWorldMut, EmptyWorldRef};

mod inner {
    use std::marker::PhantomData;

    use naia_shared::{
        DiffMask, PacketReader, ProtocolInserter, Protocolize, ReplicaDynRefWrapper,
        ReplicaMutWrapper, ReplicaRefWrapper, Replicate, ReplicateSafe, WorldMutType, WorldRefType,
    };

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
        fn has_entity(&self, _: &EmptyEntity) -> bool {
            unimplemented!()
        }

        fn entities(&self) -> Vec<EmptyEntity> {
            unimplemented!()
        }

        fn has_component<R: ReplicateSafe<P>>(&self, _: &EmptyEntity) -> bool {
            unimplemented!()
        }

        fn has_component_of_kind(&self, _: &EmptyEntity, _: &P::Kind) -> bool {
            unimplemented!()
        }

        fn get_component<R: ReplicateSafe<P>>(
            &self,
            _: &EmptyEntity,
        ) -> Option<ReplicaRefWrapper<P, R>> {
            unimplemented!()
        }

        fn get_component_of_kind(
            &self,
            _: &EmptyEntity,
            _: &P::Kind,
        ) -> Option<ReplicaDynRefWrapper<'_, P>> {
            unimplemented!()
        }
    }

    impl<P: Protocolize> WorldRefType<P, EmptyEntity> for EmptyWorldMut<P> {
        fn has_entity(&self, _: &EmptyEntity) -> bool {
            unimplemented!()
        }

        fn entities(&self) -> Vec<EmptyEntity> {
            unimplemented!()
        }

        fn has_component<R: ReplicateSafe<P>>(&self, _: &EmptyEntity) -> bool {
            unimplemented!()
        }

        fn has_component_of_kind(&self, _: &EmptyEntity, _: &P::Kind) -> bool {
            unimplemented!()
        }

        fn get_component<R: ReplicateSafe<P>>(
            &self,
            _: &EmptyEntity,
        ) -> Option<ReplicaRefWrapper<P, R>> {
            unimplemented!()
        }

        fn get_component_of_kind(
            &self,
            _: &EmptyEntity,
            _: &P::Kind,
        ) -> Option<ReplicaDynRefWrapper<'_, P>> {
            unimplemented!()
        }
    }

    impl<P: Protocolize> WorldMutType<P, EmptyEntity> for EmptyWorldMut<P> {
        fn get_component_mut<R: ReplicateSafe<P>>(
            &mut self,
            _: &EmptyEntity,
        ) -> Option<ReplicaMutWrapper<P, R>> {
            unimplemented!()
        }

        fn component_read_partial(
            &mut self,
            _: &EmptyEntity,
            _: &P::Kind,
            _: &DiffMask,
            _: &mut PacketReader,
            _: u16,
        ) {
            unimplemented!()
        }

        fn mirror_components(&mut self, _: &EmptyEntity, _: &EmptyEntity, _: &P::Kind) {
            unimplemented!()
        }

        fn get_component_kinds(&mut self, _: &EmptyEntity) -> Vec<P::Kind> {
            unimplemented!()
        }

        fn spawn_entity(&mut self) -> EmptyEntity {
            unimplemented!()
        }

        fn despawn_entity(&mut self, _: &EmptyEntity) {
            unimplemented!()
        }

        fn insert_component<R: ReplicateSafe<P>>(&mut self, _: &EmptyEntity, _: R) {
            unimplemented!()
        }

        fn remove_component<R: Replicate<P>>(&mut self, _: &EmptyEntity) -> Option<R> {
            unimplemented!()
        }

        fn remove_component_of_kind(&mut self, _: &EmptyEntity, _: &P::Kind) -> Option<P> {
            unimplemented!()
        }
    }

    impl<P: Protocolize> ProtocolInserter<P, EmptyEntity> for EmptyWorldMut<P> {
        fn insert<I: ReplicateSafe<P>>(&mut self, _: &EmptyEntity, _: I) {
            unimplemented!()
        }
    }
}
