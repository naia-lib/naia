use std::time::Duration;

use naia_shared::{
    Channel, ChannelDirection, ChannelMode, ComponentKind, CompressionConfig,
    LinkConditionerConfig, Message, Protocol as InnerProtocol, Replicate, Request,
};

use crate::{ProtocolPlugin, WorldData};

pub struct Protocol {
    inner: InnerProtocol,
    world_data: Option<WorldData>,
}

impl Default for Protocol {
    fn default() -> Self {
        Self {
            inner: InnerProtocol::default(),
            world_data: Some(WorldData::new()),
        }
    }
}

impl Protocol {
    pub fn builder() -> Self {
        Self::default()
    }

    pub fn take_world_data(&mut self) -> WorldData {
        self.world_data.take().expect("should only call this once")
    }

    pub fn add_plugin<P: ProtocolPlugin>(&mut self, plugin: P) -> &mut Self {
        self.check_lock();
        plugin.build(self);
        self
    }

    pub fn link_condition(&mut self, config: LinkConditionerConfig) -> &mut Self {
        self.inner.link_condition(config);
        self
    }

    pub fn enable_client_authoritative_entities(&mut self) -> &mut Self {
        self.inner.enable_client_authoritative_entities();
        self
    }

    pub fn rtc_endpoint(&mut self, path: String) -> &mut Self {
        self.inner.rtc_endpoint(path);
        self
    }

    pub fn tick_interval(&mut self, duration: Duration) -> &mut Self {
        self.inner.tick_interval(duration);
        self
    }

    pub fn compression(&mut self, config: CompressionConfig) -> &mut Self {
        self.inner.compression(config);
        self
    }

    pub fn add_default_channels(&mut self) -> &mut Self {
        self.inner.add_default_channels();
        self
    }

    pub fn add_channel<C: Channel>(
        &mut self,
        direction: ChannelDirection,
        mode: ChannelMode,
    ) -> &mut Self {
        self.inner.add_channel::<C>(direction, mode);
        self
    }

    pub fn add_message<M: Message>(&mut self) -> &mut Self {
        self.inner.add_message::<M>();
        self
    }

    pub fn add_request<Q: Request>(&mut self) -> &mut Self {
        self.inner.add_request::<Q>();
        self
    }

    pub fn add_component<C: Replicate>(&mut self) -> &mut Self {
        self.inner.add_component::<C>();
        self.world_data
            .as_mut()
            .expect("shouldn't happen")
            .put_kind::<C>(&ComponentKind::of::<C>());
        self
    }

    pub fn lock(&mut self) {
        self.inner.lock();
    }

    pub fn into(self) -> InnerProtocol {
        self.inner
    }

    pub fn inner(&self) -> &InnerProtocol {
        &self.inner
    }

    fn check_lock(&self) {
        self.inner.check_lock();
    }

    pub fn build(&mut self) -> Self {
        std::mem::take(self)
    }
}
