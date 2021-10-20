use std::ops::{Deref, DerefMut};

use super::{protocol_type::ProtocolType, replicate::Replicate};

// ComponentRef
pub trait ComponentRefTrait<P: ProtocolType, R: Replicate<P>> {
    fn component_deref(&self) -> &R;
}

pub struct ComponentRef<'a, P: ProtocolType, R: Replicate<P>> {
    inner: Box<dyn ComponentRefTrait<P, R> + 'a>,
}

impl<'a, P: ProtocolType, R: Replicate<P>> ComponentRef<'a, P, R> {
    pub fn new<I: ComponentRefTrait<P, R> + 'a>(inner: I) -> Self {
        return Self {
            inner: Box::new(inner),
        }
    }
}

impl<'a, P: ProtocolType, R: Replicate<P>> Deref for ComponentRef<'a, P, R> {
    type Target = R;
    fn deref(&self) -> &R {
        self.inner.component_deref()
    }
}

// ComponentMut
pub trait ComponentMutTrait<P: ProtocolType, R: Replicate<P>>: ComponentRefTrait<P, R> {
    fn component_deref_mut(&mut self) -> &mut R;
}

pub struct ComponentMut<'a, P: ProtocolType, R: Replicate<P>> {
    inner: Box<dyn ComponentMutTrait<P, R> + 'a>,
}

impl<'a, P: ProtocolType, R: Replicate<P>> ComponentMut<'a, P, R> {
    pub fn new<I: ComponentMutTrait<P, R> + 'a>(inner: I) -> Self {
        return Self {
            inner: Box::new(inner),
        }
    }
}

impl<'a, P: ProtocolType, R: Replicate<P>> Deref for ComponentMut<'a, P, R> {
    type Target = R;
    fn deref(&self) -> &R {
        self.inner.component_deref()
    }
}

impl<'a, P: ProtocolType, R: Replicate<P>> DerefMut for ComponentMut<'a, P, R> {
    fn deref_mut(&mut self) -> &mut R {
        self.inner.component_deref_mut()
    }
}

// ComponentDynRef
pub trait ComponentDynRefTrait<P: ProtocolType> {
    fn component_dyn_deref(&self) -> &dyn Replicate<P>;
}

pub struct ComponentDynRef<'a, P: ProtocolType> {
    inner: Box<dyn ComponentDynRefTrait<P> + 'a>,
}

impl<'a, P: ProtocolType> ComponentDynRef<'a, P> {
    pub fn new<I: ComponentDynRefTrait<P> + 'a>(inner: I) -> Self {
        return Self {
            inner: Box::new(inner),
        }
    }
}

impl<'a, P: ProtocolType> Deref for ComponentDynRef<'a, P> {
    type Target = dyn Replicate<P>;
    fn deref(&self) -> &dyn Replicate<P> {
        self.inner.component_dyn_deref()
    }
}

// ComponentDynMut
pub trait ComponentDynMutTrait<P: ProtocolType>: ComponentDynRefTrait<P> {
    fn component_dyn_deref_mut(&mut self) -> &mut dyn Replicate<P>;
}

pub struct ComponentDynMut<'a, P: ProtocolType> {
    inner: Box<dyn ComponentDynMutTrait<P> + 'a>,
}

impl<'a, P: ProtocolType> ComponentDynMut<'a, P> {
    pub fn new<I: ComponentDynMutTrait<P> + 'a>(inner: I) -> Self {
        return Self {
            inner: Box::new(inner),
        }
    }
}

impl<'a, P: ProtocolType> Deref for ComponentDynMut<'a, P> {
    type Target = dyn Replicate<P>;
    fn deref(&self) -> &dyn Replicate<P> {
        self.inner.component_dyn_deref()
    }
}

impl<'a, P: ProtocolType> DerefMut for ComponentDynMut<'a, P> {
    fn deref_mut(&mut self) -> &mut dyn Replicate<P> {
        self.inner.component_dyn_deref_mut()
    }
}