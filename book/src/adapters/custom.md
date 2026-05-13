# Writing Your Own Adapter

Implementing a naia adapter for a custom game framework requires implementing
two traits: `WorldMutType<E>` and `WorldRefType<E>`.

---

## `WorldMutType<E>`

Provides mutable world access that naia uses to spawn, modify, and despawn entities:

```rust
pub trait WorldMutType<E: Copy + Eq + Hash> {
    fn spawn_entity(&mut self) -> E;
    fn despawn_entity(&mut self, entity: &E);
    fn insert_component<C: Replicate>(&mut self, entity: &E, component: C);
    fn remove_component<C: Replicate>(&mut self, entity: &E) -> Option<C>;
    fn component_mut<C: Replicate>(&mut self, entity: &E) -> Option<ReplicateMut<C>>;
    // … additional methods
}
```

## `WorldRefType<E>`

Provides immutable world access for reading component values:

```rust
pub trait WorldRefType<E: Copy + Eq + Hash> {
    fn has_entity(&self, entity: &E) -> bool;
    fn component<C: Replicate>(&self, entity: &E) -> Option<&C>;
    // … additional methods
}
```

---

## Minimal adapter skeleton

```rust
pub struct MyWorld {
    entities: HashMap<u32, EntityData>,
    next_id: u32,
}

impl WorldMutType<u32> for MyWorld {
    fn spawn_entity(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.entities.insert(id, EntityData::default());
        id
    }

    fn despawn_entity(&mut self, entity: &u32) {
        self.entities.remove(entity);
    }

    // … implement remaining methods
}
```

---

> **Tip:** The best references for a minimal custom world are
> `demos/demo_utils/demo_world` and `demos/demo_utils/empty_world`. They show the
> core world traits without Bevy's adapter layer.
