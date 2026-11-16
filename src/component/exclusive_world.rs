use crate::{
    commands::Commands,
    ecs::component::ComponentBundle,
    entity::{Entity, EntityMut, EntityRef},
    events::{Event, EventReader, EventWriter},
    resources::{NonSendRes, NonSendResMut, Res, ResMut, Resource},
    world::World,
};

/// Restricted exclusive world access passed into [`ComponentHook`] callbacks.
///
/// Exposes entity inspection, immediate structural operations, resource access, and event
/// dispatch while preventing disallowed lifecycle operations (such as registering new component
/// hooks mid-flush).
///
/// [`ComponentHook`]: crate::ecs::component::hooks::ComponentHook
pub struct ExclusiveWorld<'w> {
    pub(crate) world: &'w mut World,
}

impl<'w> ExclusiveWorld<'w> {
    /// Creates a new [`ExclusiveWorld`] from a mutable [`World`] reference.
    ///
    /// [`ExclusiveWorld`]: crate::ecs::component::hooks::exclusive_world::ExclusiveWorld
    pub fn new(world: &'w mut World) -> Self {
        Self { world }
    }
    /// Returns a read-only [`EntityRef`] for inspecting components on `entity`.
    ///
    /// [`EntityRef`]: crate::ecs::entity::EntityRef
    pub fn entity<'a>(&'a self, entity: &'a Entity) -> EntityRef<'a> {
        self.world.entity(entity)
    }

    /// Returns a mutable [`EntityMut`] for inspecting and modifying components on `entity`.
    ///
    /// [`EntityMut`]: crate::ecs::entity::EntityMut
    pub fn entity_mut<'a>(&'a self, entity: &'a Entity) -> EntityMut<'a> {
        self.world.entity_mut(entity)
    }

    /// Spawns a new entity.
    ///
    /// Functions identically to [`Commands::spawn`], except structural changes take effect immediately.
    /// Refer to its documentation for more info.
    ///
    /// [`Commands::spawn`]: crate::ecs::commands::Commands::spawn
    pub fn spawn<B: ComponentBundle>(&mut self, component_bundle: B) -> Entity {
        self.world.spawn(component_bundle)
    }

    /// Spawns a batch of entities from an iterator.
    ///
    /// Functions identically to [`Commands::spawn_batch`], except structural changes take effect immediately.
    /// Refer to its documentation for more info.
    ///
    /// [`Commands::spawn_batch`]: crate::ecs::commands::Commands::spawn_batch
    pub fn spawn_batch<B, I>(&mut self, components_iter: I) -> Vec<Entity>
    where
        B: ComponentBundle,
        I: Iterator<Item = B> + Send + 'static,
    {
        self.world.spawn_batch(components_iter)
    }

    /// Inserts a bundle of components onto an existing entity.
    ///
    /// Functions identically to [`EntityCommands::insert`], except structural changes take effect immediately.
    /// Refer to its documentation for more info.
    ///
    /// [`EntityCommands::insert`]: crate::ecs::commands::EntityCommands::insert
    pub fn insert_components<B: ComponentBundle>(&mut self, entity: Entity, component_bundle: B) {
        self.world.insert_components(entity, component_bundle);
    }

    /// Removes a bundle of components from an entity.
    ///
    /// Functions identically to [`EntityCommands::remove`], except structural changes take effect immediately.
    /// Refer to its documentation for more info.
    ///
    /// [`EntityCommands::remove`]: crate::ecs::commands::EntityCommands::remove
    pub fn remove_components<B: ComponentBundle>(&mut self, entity: Entity) {
        self.world.remove_components::<B>(entity);
    }

    /// Despawns an entity.
    ///
    /// Functions identically to [`EntityCommands::despawn`], except structural changes take effect immediately.
    /// Refer to its documentation for more info.
    ///
    /// [`EntityCommands::despawn`]: crate::ecs::commands::EntityCommands::despawn
    pub fn despawn(&mut self, entity: Entity) {
        self.world.despawn(entity)
    }

    /// Returns a [`Commands`] buffer for queueing deferred structural changes to the world.
    ///
    /// [`Commands`]: crate::ecs::commands::Commands
    pub fn commands<'a>(&'a self) -> Commands<'a> {
        self.world.commands()
    }

    /// Returns `true` if a thread-safe [`Resource`] of type `T` exists in the world.
    ///
    /// [`Resource`]: crate::ecs::resources::Resource
    pub fn contains_resource<T: Resource + Send + Sync>(&self) -> bool {
        self.world.contains_resource::<T>()
    }

    /// Inserts a thread-safe [`Resource`] of type `T` into the world, returning the previous value
    /// if one existed.
    ///
    /// [`Resource`]: crate::ecs::resources::Resource
    pub fn insert_resource<T: Resource + Send + Sync>(&self, resource: T) -> Option<T> {
        self.world.insert_resource(resource)
    }

    /// Removes and returns the thread-safe [`Resource`] of type `T` from the world, if present.
    ///
    /// [`Resource`]: crate::ecs::resources::Resource
    pub fn remove_resource<T: Resource + Send + Sync>(&self) -> Option<T> {
        self.world.remove_resource::<T>()
    }

    /// Returns a shared [`Res`] guard to the thread-safe [`Resource`] of type `R`.
    ///
    /// # Panics
    /// Panics if no resource of type `R` exists in the world. Use [`ExclusiveWorld::get_resource`]
    /// for a non-panicking alternative.
    ///
    /// [`Res`]: crate::ecs::resources::Res
    /// [`Resource`]: crate::ecs::resources::Resource
    /// [`ExclusiveWorld::get_resource`]: Self::get_resource
    pub fn resource<'a, R: Resource + Send + Sync>(&'a self) -> Res<'a, R> {
        self.world.resource::<R>()
    }

    /// Returns an exclusive [`ResMut`] guard to the thread-safe [`Resource`] of type `R`.
    ///
    /// # Panics
    /// Panics if no resource of type `R` exists in the world. Use
    /// [`ExclusiveWorld::get_resource_mut`] for a non-panicking alternative.
    ///
    /// [`ResMut`]: crate::ecs::resources::ResMut
    /// [`Resource`]: crate::ecs::resources::Resource
    /// [`ExclusiveWorld::get_resource_mut`]: Self::get_resource_mut
    pub fn resource_mut<'a, R: Resource + Send + Sync>(&'a self) -> ResMut<'a, R> {
        self.world.resource_mut::<R>()
    }

    /// Returns a shared [`Res`] guard to the thread-safe [`Resource`] of type `R`, or `None` if it
    /// does not exist.
    ///
    /// [`Res`]: crate::ecs::resources::Res
    /// [`Resource`]: crate::ecs::resources::Resource
    pub fn get_resource<'a, R: Resource + Send + Sync>(&'a self) -> Option<Res<'a, R>> {
        self.world.get_resource::<R>()
    }

    /// Returns an exclusive [`ResMut`] guard to the thread-safe [`Resource`] of type `R`, or `None`
    /// if it does not exist.
    ///
    /// [`ResMut`]: crate::ecs::resources::ResMut
    /// [`Resource`]: crate::ecs::resources::Resource
    pub fn get_resource_mut<'a, R: Resource + Send + Sync>(&'a self) -> Option<ResMut<'a, R>> {
        self.world.get_resource_mut::<R>()
    }

    /// Returns `true` if a main-thread (`NonSend`) [`Resource`] of type `T` exists in the world.
    ///
    /// [`Resource`]: crate::ecs::resources::Resource
    pub fn contains_non_send_resource<T: Resource>(&self) -> bool {
        self.world.contains_non_send_resource::<T>()
    }

    /// Inserts a main-thread (`NonSend`) [`Resource`] of type `T` into the world, returning the
    /// previous value if one existed.
    ///
    /// [`Resource`]: crate::ecs::resources::Resource
    pub fn insert_non_send_resource<T: Resource>(&mut self, resource: T) -> Option<T> {
        self.world.insert_non_send_resource(resource)
    }

    /// Removes and returns the main-thread (`NonSend`) [`Resource`] of type `T` from the world,
    /// if present.
    ///
    /// [`Resource`]: crate::ecs::resources::Resource
    pub fn remove_non_send_resource<T: Resource>(&mut self) -> Option<T> {
        self.world.remove_non_send_resource::<T>()
    }

    /// Returns a shared [`NonSendRes`] reference to the main-thread [`Resource`] of type `R`.
    ///
    /// # Panics
    /// Panics if no resource of type `R` exists in the world. Use
    /// [`ExclusiveWorld::get_non_send_resource`] for a non-panicking alternative.
    ///
    /// [`NonSendRes`]: crate::ecs::resources::NonSendRes
    /// [`Resource`]: crate::ecs::resources::Resource
    /// [`ExclusiveWorld::get_non_send_resource`]: Self::get_non_send_resource
    pub fn non_send_resource<'a, R: Resource>(&'a self) -> NonSendRes<'a, R> {
        self.world.non_send_resource::<R>()
    }

    /// Returns an exclusive [`NonSendResMut`] reference to the main-thread [`Resource`] of type `R`.
    ///
    /// # Panics
    /// Panics if no resource of type `R` exists in the world. Use
    /// [`ExclusiveWorld::get_non_send_resource_mut`] for a non-panicking alternative.
    ///
    /// [`NonSendResMut`]: crate::ecs::resources::NonSendResMut
    /// [`Resource`]: crate::ecs::resources::Resource
    /// [`ExclusiveWorld::get_non_send_resource_mut`]: Self::get_non_send_resource_mut
    pub fn non_send_resource_mut<'a, R: Resource>(&'a mut self) -> NonSendResMut<'a, R> {
        self.world.non_send_resource_mut::<R>()
    }

    /// Returns a shared [`NonSendRes`] reference to the main-thread [`Resource`] of type `R`, or
    /// `None` if it does not exist.
    ///
    /// [`NonSendRes`]: crate::ecs::resources::NonSendRes
    /// [`Resource`]: crate::ecs::resources::Resource
    pub fn get_non_send_resource<'a, R: Resource>(&'a self) -> Option<NonSendRes<'a, R>> {
        self.world.get_non_send_resource::<R>()
    }

    /// Returns an exclusive [`NonSendResMut`] reference to the main-thread [`Resource`] of type
    /// `R`, or `None` if it does not exist.
    ///
    /// [`NonSendResMut`]: crate::ecs::resources::NonSendResMut
    /// [`Resource`]: crate::ecs::resources::Resource
    pub fn get_non_send_resource_mut<'a, R: Resource>(
        &'a mut self,
    ) -> Option<NonSendResMut<'a, R>> {
        self.world.get_non_send_resource_mut::<R>()
    }

    /// Retrieve a mutable reference to a non-send resource using a shared reference.
    ///
    /// # Safety
    /// The caller must ensure that no other mutable references to the same `NonSendRes`
    /// resource are active at the same time. Aliasing a mutable reference triggers
    /// immediate undefined behavior.
    pub unsafe fn non_send_resource_mut_unsafe<'a, R: Resource>(&'a self) -> NonSendResMut<'a, R> {
        unsafe { self.world.non_send_resource_mut_unsafe::<R>() }
    }

    /// Safely attempt to retrieve a mutable reference to a non-send resource using a shared reference.
    /// Returns `None` if the resource does not exist.
    ///
    /// # Safety
    /// The caller must ensure that no other mutable references to the same `NonSendRes`
    /// resource are active at the same time.
    pub unsafe fn get_non_send_resource_mut_unsafe<'a, R: Resource>(
        &'a self,
    ) -> Option<NonSendResMut<'a, R>> {
        unsafe { self.world.get_non_send_resource_mut_unsafe::<R>() }
    }

    /// Returns an [`EventWriter`] for dispatching events of type `E`.
    ///
    /// [`EventWriter`]: crate::ecs::events::EventWriter
    pub fn event_writer<'a, E: Event>(&'a self) -> EventWriter<'a, E> {
        self.world.event_writer()
    }

    /// Returns an [`EventReader`] for reading events of type `E`.
    ///
    /// [`EventReader`]: crate::ecs::events::EventReader
    pub fn event_reader<'a, E: Event>(&'a self) -> EventReader<'a, E> {
        self.world.event_reader()
    }
}

impl<'w> From<&'w mut World> for ExclusiveWorld<'w> {
    fn from(value: &'w mut World) -> Self {
        Self::new(value)
    }
}
