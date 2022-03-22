use super::protocolize::Protocolize;
use crate::NetEntityHandleConverter;
use naia_serde::BitReader;

/// Handles the creation of new Replica (Message/Component) instances
pub trait ReplicaBuilder<P: Protocolize>: Send + Sync + ReplicaBuilderClone<P> {
    /// Create a new Replica instance
    fn build(&self, reader: &mut BitReader, converter: &dyn NetEntityHandleConverter) -> P;
    /// Gets the ProtocolKind of the Replica the builder is able to build
    fn kind(&self) -> P::Kind;
}

pub trait ReplicaBuilderClone<P: Protocolize> {
    fn clone_box(&self) -> Box<dyn ReplicaBuilder<P>>;
}

impl<P: Protocolize, T> ReplicaBuilderClone<P> for T
where
    T: 'static + ReplicaBuilder<P> + Clone,
{
    fn clone_box(&self) -> Box<dyn ReplicaBuilder<P>> {
        Box::new(self.clone())
    }
}

impl<P: Protocolize> Clone for Box<dyn ReplicaBuilder<P>> {
    fn clone(&self) -> Box<dyn ReplicaBuilder<P>> {
        self.clone_box()
    }
}
