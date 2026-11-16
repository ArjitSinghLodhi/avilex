use rustc_hash::FxHashMap;
use std::{any::TypeId, marker::PhantomData, rc::Rc};

use crate::{
    component::exclusive_world::ExclusiveWorld, ecs::component::Component, entity::Entity,
};

/// Function pointer type executed during [`Component`] lifecycle events (`on_add`, `on_insert`,
/// `on_discard`, or `on_remove`).
///
/// [`Component`]: crate::ecs::component::Component
pub type ComponentHook = fn(&mut ExclusiveWorld, &Entity);

#[derive(Clone, Default)]
pub(crate) struct ComponentHooks {
    pub(crate) on_add: Rc<[ComponentHook]>,
    pub(crate) on_insert: Rc<[ComponentHook]>,
    pub(crate) on_remove: Rc<[ComponentHook]>,
    pub(crate) on_discard: Rc<[ComponentHook]>,
}

pub(crate) struct ComponentHooksRegistry {
    pub(crate) hooks: FxHashMap<TypeId, ComponentHooks>,
    auto_registered: FxHashMap<TypeId, bool>,
}

impl ComponentHooksRegistry {
    pub(crate) fn new() -> Self {
        Self {
            hooks: FxHashMap::default(),
            auto_registered: FxHashMap::default(),
        }
    }

    pub(crate) fn prepopulate_hooks<T: Component>(&mut self) {
        let type_id = TypeId::of::<T>();

        if self.hooks.contains_key(&type_id) {
            return;
        }

        self.hooks.insert(type_id, ComponentHooks::default());
        self.auto_registered.insert(type_id, false);
    }

    pub(crate) fn register_hooks<'a, T: Component>(
        &'a mut self,
    ) -> ComponentHooksRegisterer<'a, T> {
        self.prepopulate_hooks::<T>();
        ComponentHooksRegisterer::new(self)
    }

    pub(crate) fn get_hooks_or_init<C: Component>(&mut self) -> ComponentHooks {
        let type_id = TypeId::of::<C>();

        let is_auto = self.auto_registered.get(&type_id).copied().unwrap_or(false);
        if is_auto && let Some(hooks) = self.hooks.get(&type_id) {
            return hooks.clone();
        }

        let entry = self.hooks.entry(type_id).or_default();

        let mut on_add_vec = entry.on_add.to_vec();
        let mut on_insert_vec = entry.on_insert.to_vec();
        let mut on_remove_vec = entry.on_remove.to_vec();
        let mut on_discard_vec = entry.on_discard.to_vec();

        if let Some(h) = C::on_add() {
            on_add_vec.push(h);
        }
        if let Some(h) = C::on_insert() {
            on_insert_vec.push(h);
        }
        if let Some(h) = C::on_remove() {
            on_remove_vec.push(h);
        }
        if let Some(h) = C::on_discard() {
            on_discard_vec.push(h);
        }

        entry.on_add = Rc::from(on_add_vec);
        entry.on_insert = Rc::from(on_insert_vec);
        entry.on_remove = Rc::from(on_remove_vec);
        entry.on_discard = Rc::from(on_discard_vec);

        self.auto_registered.insert(type_id, true);
        entry.clone()
    }

    pub(crate) fn get_hooks(&self, type_id: TypeId) -> Option<ComponentHooks> {
        self.hooks.get(&type_id).cloned()
    }

    pub(crate) fn is_auto_registered(&self, type_id: TypeId) -> bool {
        self.auto_registered.get(&type_id).copied().unwrap_or(false)
    }
}

/// Builder returned by [`World::register_component_hooks`] for registering
/// manual lifecycle [`ComponentHook`] callbacks on component `T`.
///
/// Manual hooks are appended after any trait-level hooks defined on [`Component`], preserving
/// baseline trait behavior.
///
/// [`World::register_component_hooks`]: crate::ecs::world::World::register_component_hooks
/// [`ComponentHook`]: crate::ecs::component::hooks::ComponentHook
/// [`Component`]: crate::ecs::component::Component
pub struct ComponentHooksRegisterer<'a, T: Component> {
    registry: &'a mut ComponentHooksRegistry,
    _marker: PhantomData<T>,
}

impl<'a, T: Component> ComponentHooksRegisterer<'a, T> {
    pub(crate) fn new(registry: &'a mut ComponentHooksRegistry) -> Self {
        Self {
            registry,
            _marker: PhantomData,
        }
    }

    /// Appends a [`ComponentHook`] executed when component `T` is newly added to an entity.
    ///
    /// [`ComponentHook`]: crate::ecs::component::hooks::ComponentHook
    pub fn on_add(&mut self, hook: ComponentHook) -> &mut Self {
        let entry = self.registry.hooks.get_mut(&TypeId::of::<T>()).unwrap();
        let mut current = entry.on_add.to_vec();
        current.push(hook);
        entry.on_add = Rc::from(current);
        self
    }

    /// Appends a [`ComponentHook`] executed whenever component `T` is inserted or overwritten on
    /// an entity.
    ///
    /// [`ComponentHook`]: crate::ecs::component::hooks::ComponentHook
    pub fn on_insert(&mut self, hook: ComponentHook) -> &mut Self {
        let entry = self.registry.hooks.get_mut(&TypeId::of::<T>()).unwrap();
        let mut current = entry.on_insert.to_vec();
        current.push(hook);
        entry.on_insert = Rc::from(current);
        self
    }

    /// Appends a [`ComponentHook`] executed right before an existing instance of component `T` is
    /// overwritten.
    ///
    /// [`ComponentHook`]: crate::ecs::component::hooks::ComponentHook
    pub fn on_discard(&mut self, hook: ComponentHook) -> &mut Self {
        let entry = self.registry.hooks.get_mut(&TypeId::of::<T>()).unwrap();
        let mut current = entry.on_discard.to_vec();
        current.push(hook);
        entry.on_discard = Rc::from(current);
        self
    }

    /// Appends a [`ComponentHook`] executed when component `T` is removed from an entity or
    /// dropped during despawn.
    ///
    /// [`ComponentHook`]: crate::ecs::component::hooks::ComponentHook
    pub fn on_remove(&mut self, hook: ComponentHook) -> &mut Self {
        let entry = self.registry.hooks.get_mut(&TypeId::of::<T>()).unwrap();
        let mut current = entry.on_remove.to_vec();
        current.push(hook);
        entry.on_remove = Rc::from(current);
        self
    }
}
