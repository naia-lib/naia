use std::collections::HashSet;

use crate::{ComponentKind, Replicate};

pub trait ReplicateBundle: Send + Sync + 'static {
    fn kind_set() -> HashSet<ComponentKind>;
}

macro_rules! impl_reflect_tuple {
    {$($name:tt),*} => {
        impl<$($name : Replicate,)*> ReplicateBundle for ($($name,)*) {
            fn kind_set() -> HashSet<ComponentKind> {
                let mut set = HashSet::new();
                $(set.insert(ComponentKind::of::<$name>());)*
                set
            }
        }
    }
}

// up to 8 tuples
impl_reflect_tuple! {A, B}
impl_reflect_tuple! {A, B, C}
impl_reflect_tuple! {A, B, C, D}
impl_reflect_tuple! {A, B, C, D, E}
impl_reflect_tuple! {A, B, C, D, E, F}
impl_reflect_tuple! {A, B, C, D, E, F, G}
impl_reflect_tuple! {A, B, C, D, E, F, G, H}
