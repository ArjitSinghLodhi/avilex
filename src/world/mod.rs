pub(crate) mod archetypes;

use rustc_hash::FxHashMap;
use std::{
    any::{Any, TypeId, type_name},
    cell::UnsafeCell,
    marker::PhantomData,
    sync::Arc,
};

use crate::{
    app_impl::APP_BUILT,
    commands::{
        Command, CommandBuffer, Commands, DespawnCommand, ParallelCommands,
        command_types::{
            BatchSpawnCommandCore, InsertComponentsCommand, RemoveComponentsCommand,
            SpawnCommandCore,
        },
    },
    component::{
        Component,
        bundle::ComponentBundle,
        hooks::{ComponentHooksRegisterer, ComponentHooksRegistry},
    },
    ecs::query::ParallelQueryAccessor,
    entity::{ENTITY_HANDLE_COUNT_REGISTRY, Entity, EntityMut, EntityRef},
    events::{
        Event, EventReader, EventWriter, ParallelEventReader, ParallelEventWriter, TRACKED_EVENTS,
    },
    query::{QueryData, QueryFilter},
    reactivity::{TRACKED_COMPONENTS, removed::REMOVAL_TRACKED_COMPS},
    resources::{
        ConcurrentResourceRegistry, NonSendRes, NonSendResMut, ParallelResourceAccessor, Res,
        ResMut, Resource,
    },
    states::StateTransitionSystems,
    system::{SystemAccessMeta, SystemParam},
    world::archetypes::ArchetypeManager,
};

/// The central ECS storage container holding all [`Entity`] archetypes, global [`Resource`]s,
/// component lifecycle hooks, and deferred [`Commands`] queues.
///
/// # Architecture & Thread Safety
/// `World` separates thread-safe data from main-thread-only data at the type level:
/// - **Concurrent Resources (`T: Resource + Send + Sync`):** Stored inside a lock-guarded
///   registry allowing shared read ([`Res`]) and exclusive write ([`ResMut`]) access from `&self`
///   across parallel systems and out-of-band worker threads.
/// - **Main-Thread Resources (`NonSend`):** Stored separately without `Send + Sync` bounds.
///   Mutable insertion, removal, and borrowing require `&mut World`, ensuring thread-unsafe
///   types (such as window handles or OpenGL contexts) never cross thread boundaries.
/// - **Out-of-Band Parallel Handles:** Use [`World::par_commands`],
///   [`World::par_resource_accessor`], [`World::par_query_accessor`],
///   [`World::par_event_writer`], and [`World::par_event_reader`] to extract thread-safe remotes
///   for background tasks.
///
/// [`World::par_commands`]: Self::par_commands
/// [`World::par_resource_accessor`]: Self::par_resource_accessor
/// [`World::par_query_accessor`]: Self::par_query_accessor
/// [`World::par_event_writer`]: Self::par_event_writer
/// [`World::par_event_reader`]: Self::par_event_reader
pub struct World {
    pub(crate) archetypes_manager: ArchetypeManager,
    pub(crate) resources: Arc<ConcurrentResourceRegistry>,
    pub(crate) non_send_resources: FxHashMap<TypeId, UnsafeCell<Box<dyn Any>>>,
    pub(crate) command_buffer: CommandBuffer,
    pub(crate) free_indices_list: Vec<u32>,
    pub(crate) component_hooks_registry: ComponentHooksRegistry,
}

unsafe impl Sync for World {}

impl World {
    pub(crate) fn new() -> Self {
        Self {
            archetypes_manager: ArchetypeManager::new(),
            resources: Arc::new(ConcurrentResourceRegistry::new()),
            non_send_resources: FxHashMap::default(),
            command_buffer: CommandBuffer::new(),
            free_indices_list: Vec::new(),
            component_hooks_registry: ComponentHooksRegistry::new(),
        }
    }

    /// Returns `true` if a thread-safe [`Resource`] of type `T` currently exists in the world.
    pub fn contains_resource<T: Resource + Send + Sync>(&self) -> bool {
        self.resources.contains_resource::<T>()
    }

    /// Inserts a thread-safe [`Resource`] of type `T` into the world.
    ///
    /// If a resource of type `T` already existed, it is replaced and the previous value is returned.
    /// Because concurrent resources use internal synchronization, this only requires `&self`.
    pub fn insert_resource<T: Resource + Send + Sync>(&self, resource: T) -> Option<T> {
        self.resources.insert_resource(resource)
    }

    /// Removes and returns the thread-safe [`Resource`] of type `T` from the world, if present.
    pub fn remove_resource<T: Resource + Send + Sync>(&self) -> Option<T> {
        self.resources.remove_resource::<T>()
    }

    /// Acquires a shared, read-only guard ([`Res<'w, T>`]) to a thread-safe [`Resource`] of type `T`.
    ///
    /// # Panics
    /// Panics if no resource of type `T` exists in the world. Use [`World::get_resource`] for a
    /// non-panicking alternative.
    ///
    /// # Deadlocks
    /// Blocks if another thread holds a [`ResMut<'w, T>`] guard for `T`, and deadlocks if the
    /// **same** thread already holds a [`ResMut<'w, T>`] guard for `T`.
    ///
    /// [`Res<'w, T>`]: crate::resources::Res
    /// [`ResMut<'w, T>`]: crate::resources::ResMut
    /// [`World::get_resource`]: Self::get_resource
    pub fn resource<'w, T: Resource + Send + Sync>(&'w self) -> Res<'w, T> {
        self.resources.resource::<T>()
    }

    /// Acquires an exclusive, mutable guard ([`ResMut<'w, T>`]) to a thread-safe [`Resource`] of type `T`.
    ///
    /// # Panics
    /// Panics if no resource of type `T` exists in the world. Use [`World::get_resource_mut`] for
    /// a non-panicking alternative.
    ///
    /// # Deadlocks
    /// Blocks until active [`Res<'w, T>`] or [`ResMut<'w, T>`] guards are released, and deadlocks
    /// if the **same** thread already holds any borrow of `T`.
    ///
    /// [`Res<'w, T>`]: crate::resources::Res
    /// [`ResMut<'w, T>`]: crate::resources::ResMut
    /// [`World::get_resource_mut`]: Self::get_resource_mut
    pub fn resource_mut<'w, T: Resource + Send + Sync>(&'w self) -> ResMut<'w, T> {
        self.resources.resource_mut::<T>()
    }

    /// Attempts to acquire a shared, read-only guard ([`Res<'w, T>`]) to a thread-safe [`Resource`]
    /// of type `T`, returning `None` if it is not present.
    ///
    /// [`Res<'w, T>`]: crate::resources::Res
    pub fn get_resource<'w, T: Resource + Send + Sync>(&'w self) -> Option<Res<'w, T>> {
        self.resources.get_resource::<T>()
    }

    /// Attempts to acquire an exclusive, mutable guard ([`ResMut<'w, T>`]) to a thread-safe [`Resource`]
    /// of type `T`, returning `None` if it is not present.
    ///
    /// [`ResMut<'w, T>`]: crate::resources::ResMut
    pub fn get_resource_mut<'w, T: Resource + Send + Sync>(&'w self) -> Option<ResMut<'w, T>> {
        self.resources.get_resource_mut::<T>()
    }

    /// Returns `true` if a main-thread (`NonSend`) [`Resource`] of type `T` exists in the world.
    pub fn contains_non_send_resource<T: Resource>(&self) -> bool {
        self.non_send_resources.contains_key(&TypeId::of::<T>())
    }

    /// Inserts a main-thread (`NonSend`) [`Resource`] of type `T` into the world.
    ///
    /// Unlike [`World::insert_resource`], `T` does not need to implement `Send` or `Sync`. Requires
    /// `&mut self` to guarantee exclusive main-thread access. Returns the previous value if one
    /// was already present.
    ///
    /// [`World::insert_resource`]: Self::insert_resource
    pub fn insert_non_send_resource<T: Resource>(&mut self, resource: T) -> Option<T> {
        let type_id = std::any::TypeId::of::<T>();
        let boxed_cell = std::cell::UnsafeCell::new(Box::new(resource) as Box<dyn std::any::Any>);
        if let Some(res) = self.non_send_resources.insert(type_id, boxed_cell) {
            let res = res.into_inner().downcast::<T>().unwrap();
            Some(*res)
        } else {
            None
        }
    }

    /// Removes and returns the main-thread (`NonSend`) [`Resource`] of type `T`, if present.
    pub fn remove_non_send_resource<T: Resource>(&mut self) -> Option<T> {
        let type_id = TypeId::of::<T>();
        if let Some(res) = self.non_send_resources.remove(&type_id) {
            let res = res.into_inner().downcast::<T>().unwrap();
            Some(*res)
        } else {
            None
        }
    }

    /// Attempts to acquire a shared reference ([`NonSendRes<'w, T>`]) to a main-thread (`NonSend`)
    /// [`Resource`] of type `T`, returning `None` if it does not exist.
    ///
    /// [`NonSendRes<'w, T>`]: crate::resources::NonSendRes
    pub fn get_non_send_resource<'w, T: Resource>(&'w self) -> Option<NonSendRes<'w, T>> {
        let type_id = TypeId::of::<T>();
        let cell = self.non_send_resources.get(&type_id)?;

        unsafe {
            let base_any = &*cell.get();
            let casted_ref = base_any.downcast_ref::<T>()?;
            Some(NonSendRes { res: casted_ref })
        }
    }

    /// Acquires a shared reference ([`NonSendRes<'w, T>`]) to a main-thread (`NonSend`) [`Resource`]
    /// of type `T`.
    ///
    /// # Panics
    /// Panics with `NonSendResResourceMissingError` if no resource of type `T` exists in the world.
    /// Use [`World::get_non_send_resource`] for a non-panicking alternative.
    ///
    /// [`NonSendRes<'w, T>`]: crate::resources::NonSendRes
    /// [`World::get_non_send_resource`]: Self::get_non_send_resource
    pub fn non_send_resource<'w, T: Resource>(&'w self) -> NonSendRes<'w, T> {
        self.get_non_send_resource::<T>().unwrap_or_else(|| {
            panic!(
                "NonSendResResourceMissingError\n\
                Name: {}",
                type_name::<T>()
            );
        })
    }

    /// Attempts to acquire an exclusive, mutable reference ([`NonSendResMut<'w, T>`]) to a main-thread
    /// (`NonSend`) [`Resource`] of type `T`, returning `None` if it does not exist.
    ///
    /// Requires `&mut self` to statically enforce Rust's aliasing rules without runtime locks.
    ///
    /// [`NonSendResMut<'w, T>`]: crate::resources::NonSendResMut
    pub fn get_non_send_resource_mut<'w, T: Resource>(
        &'w mut self,
    ) -> Option<NonSendResMut<'w, T>> {
        unsafe { self.get_non_send_resource_mut_unsafe::<T>() }
    }

    /// Acquires an exclusive, mutable reference ([`NonSendResMut<'w, T>`]) to a main-thread
    /// (`NonSend`) [`Resource`] of type `T`.
    ///
    /// # Panics
    /// Panics if no resource of type `T` exists in the world. Use
    /// [`World::get_non_send_resource_mut`] for a non-panicking alternative.
    ///
    /// [`NonSendResMut<'w, T>`]: NonSendResMut
    /// [`World::get_non_send_resource_mut`]: Self::get_non_send_resource_mut
    pub fn non_send_resource_mut<'w, T: Resource>(&'w mut self) -> NonSendResMut<'w, T> {
        unsafe { self.non_send_resource_mut_unsafe::<T>() }
    }

    /// Applies all queued [`Commands`] to the world.
    ///
    /// Despawns are handled separately by [`World::apply_despawns`].
    ///
    /// [`Commands`]: crate::commands::Commands
    /// [`World::apply_despawns`]: Self::apply_despawns
    pub fn apply_queue_commands(&mut self) {
        let queue_write_arc = self.command_buffer.queue_write.clone();
        let queue_apply_arc = self.command_buffer.queue_apply.clone();
        loop {
            let mut queue_apply_guard = queue_apply_arc.write();
            {
                let mut queue_write_guard = queue_write_arc.write();
                std::mem::swap(&mut *queue_write_guard, &mut *queue_apply_guard);
            }
            queue_apply_guard.apply(self);
            if queue_write_arc.read().is_empty() {
                break;
            }
        }
    }

    /// Despawns all queued entities and frees their [`Entity`] slots.
    ///
    /// [`Entity`]: crate::entity::Entity
    pub fn apply_despawns(&mut self) {
        let despawns_arc = self.command_buffer.despawns.clone();
        for meta in REMOVAL_TRACKED_COMPS.lock().values() {
            (meta.clear_dead_entities)(self, &despawns_arc)
        }
        loop {
            despawns_arc.drain(|entity| {
                let cmd = DespawnCommand { entity };
                cmd.apply(self);
            });
            if despawns_arc.is_empty() {
                break;
            }
        }
    }

    /// Swaps and clears component reactivity trackers and event buffers at the end of a frame.
    ///
    /// Called automatically by [`App::update`].
    ///
    /// [`App::update`]: crate::app::App::update
    pub fn end_of_frame_sync(&mut self) {
        self.clear_trackers();
        self.clear_events();
    }

    /// Enables [`ChangedState`] tracking for component `T`.
    ///
    /// Call this before [`App::build`] if `T` isn't already tracked automatically.
    ///
    /// [`ChangedState`]: crate::reactivity::changed::ChangedState
    /// [`App::build`]: crate::app::App::build
    pub fn register_changed_trackers<T: Component>(&self) {
        crate::reactivity::changed::register_changed_tracked_component::<T>();
    }

    /// Enables [`AddedState`] tracking for component `T`.
    ///
    /// Call this before [`App::build`] if `T` isn't already tracked automatically.
    ///
    /// [`AddedState`]: crate::reactivity::added::AddedState
    /// [`App::build`]: crate::app::App::build
    pub fn register_added_trackers<T: Component>(&self) {
        crate::reactivity::added::register_added_tracked_component::<T>();
    }

    /// Enables [`RemovedComponents`] tracking for component `T`.
    ///
    /// Call this before [`App::build`] if `T` isn't already tracked automatically.
    ///
    /// [`RemovedComponents`]: crate::reactivity::removed::RemovedComponents
    /// [`App::build`]: crate::app::App::build
    pub fn register_removal_trackers<T: Component>(&self) {
        crate::reactivity::removed::register_removal_tracking_buffers(self);
    }

    /// Synchronizes world state at the beginning of a frame before any schedules execute,
    /// applying any pending [`States`] transitions.
    ///
    /// Called automatically by [`App::update`].
    ///
    /// [`States`]: crate::app::states::States
    /// [`App::update`]: crate::app::App::update
    pub fn start_frame_sync(&mut self) {
        self.transition_states();
    }

    fn clear_trackers(&mut self) {
        let tracked_comps = TRACKED_COMPONENTS.lock();
        if !tracked_comps.is_empty() {
            for archetype in self.archetypes_manager.iter_archetypes() {
                let archetype_lock = archetype.lock();
                unsafe {
                    for meta_list in tracked_comps.values() {
                        for meta in meta_list {
                            if let Some(marker_column) = archetype_lock.columns.get(&meta.marker_id)
                            {
                                let mut guard = marker_column.data.write();
                                let raw_any = guard.as_any_mut();
                                (meta.clear_column_states)(raw_any);
                            }
                        }
                    }
                }
            }
        }

        let removal_track = REMOVAL_TRACKED_COMPS.lock();
        if !removal_track.is_empty() {
            for remove_meta in removal_track.values() {
                (remove_meta.swap_and_clear_buffer)(self)
            }
        }
    }

    fn clear_events(&mut self) {
        let tracked_events = TRACKED_EVENTS.read();
        if tracked_events.is_empty() {
            return;
        }
        for meta in tracked_events.iter() {
            let entry = self
                .resources
                .resources
                .get(&meta.event_id)
                .expect("Registered event Not initialized somehow? maybe removed");
            let mut res = entry.value().write();
            (meta.clear_events)(&mut res);
        }
    }

    fn transition_states(&mut self) {
        let state_systems_opt = self.get_resource_mut::<StateTransitionSystems>();
        if let Some(mut state_systems) = state_systems_opt {
            state_systems.systems.iter_mut().for_each(|sys| {
                sys.run((), self)
                    .expect("State transition system should not fail.")
            });
        }
    }

    /// Returns a thread-safe, thread-clonable [`ParallelCommands`] handle.
    ///
    /// This method can be called directly on the `World` to obtain a detached,
    /// safe remote input into the engine's command pipeline, manageable by external or
    /// parallel background worker threads.
    pub fn par_commands(&self) -> ParallelCommands {
        ParallelCommands::get_param(self).unwrap()
    }

    /// Returns a thread-safe, thread-clonable [`ParallelEventWriter`] handle.
    ///
    /// This method can be called directly on the `World` to obtain a detached,
    /// safe remote output channel into the engine's event queue, manageable by external
    /// or parallel background worker threads.
    pub fn par_event_writer<T: Event>(&self) -> ParallelEventWriter<T> {
        ParallelEventWriter::get_param(self).unwrap()
    }

    /// Returns a thread-safe, thread-clonable [`ParallelEventReader`] handle.
    ///
    /// This method can be called directly on the `World` to obtain a detached,
    /// safe remote input channel to inspect the engine's event queue from external or
    /// parallel background worker threads.
    pub fn par_event_reader<T: Event>(&self) -> ParallelEventReader<T> {
        ParallelEventReader::get_param(self).unwrap()
    }

    /// Returns a thread-safe, thread-clonable [`ParallelResourceAccessor`] handle.
    ///
    /// This method can be called directly on the `World` to obtain a detached,
    /// safe remote accessor into the engine's resource tables manageable by external or
    /// parallel background threads.
    pub fn par_resource_accessor<T: Resource + Send + Sync>(&self) -> ParallelResourceAccessor<T> {
        ParallelResourceAccessor::get_param(self).unwrap()
    }

    /// Returns a thread-safe [`ParallelQueryAccessor<Q, F>`] handle for querying matching
    /// components out-of-band (such as from background threads or async tasks).
    ///
    /// If `Q` or `F` uses reactive tracking (such as [`Changed<T>`] or [`Added<T>`]), call this
    /// method before [`App::build`] so the required component trackers are automatically
    /// registered, or register them manually on [`App`] or [`World`] beforehand.
    ///
    /// [`ParallelQueryAccessor<Q, F>`]: crate::ecs::query::ParallelQueryAccessor
    /// [`Changed<T>`]: crate::ecs::reactivity::changed::Changed
    /// [`Added<T>`]: crate::ecs::reactivity::added::Added
    /// [`App::build`]: crate::app::App::build
    /// [`App`]: crate::app::App
    /// [`World`]: crate::ecs::world::World
    pub fn par_query_accessor<Q: QueryData, F: QueryFilter>(&self) -> ParallelQueryAccessor<Q, F> {
        ParallelQueryAccessor::<Q, F>::init_access(&mut SystemAccessMeta::new(
            "par_query_accessor".to_string(),
        ));
        ParallelQueryAccessor::<Q, F>::get_param(self).unwrap()
    }
    /// Returns an [`EventWriter`] for sending events of type `E`.
    ///
    /// [`EventWriter`]: EventWriter
    pub fn event_writer<'w, E: Event>(&'w self) -> EventWriter<'w, E> {
        EventWriter::get_param(self).unwrap()
    }

    /// Returns an [`EventReader`] for reading events of type `E`.
    ///
    /// [`EventReader`]: EventReader
    pub fn event_reader<'w, E: Event>(&'w self) -> EventReader<'w, E> {
        EventReader::get_param(self).unwrap()
    }

    /// Returns a [`Commands`] buffer for queueing deferred structural changes to the world.
    ///
    /// [`Commands`]: Commands
    pub fn commands<'a>(&'a self) -> Commands<'a> {
        Commands::get_param(self).unwrap()
    }

    /// Returns a read-only [`EntityRef`] for inspecting components on `entity`.
    ///
    /// [`EntityRef`]: EntityRef
    pub fn entity<'w>(&'w self, entity: &'w Entity) -> EntityRef<'w> {
        EntityRef {
            world: self,
            entity,
        }
    }

    /// Returns a mutable [`EntityMut`] for inspecting and modifying components on `entity`.
    ///
    /// [`EntityMut`]: EntityMut
    pub fn entity_mut<'w>(&'w self, entity: &'w Entity) -> EntityMut<'w> {
        EntityMut {
            entity_ref: self.entity(entity),
        }
    }

    /// Returns a [`ComponentHooksRegisterer`] for registering lifecycle hooks on component `T`.
    ///
    /// # Panics
    /// Panics if called after [`App::build`] has already run, or if any entity with component `T`
    /// has already been spawned.
    ///
    /// [`ComponentHooksRegisterer`]: ComponentHooksRegisterer
    /// [`App::build`]: crate::app::App::build
    pub fn register_component_hooks<'w, T: Component>(
        &'w mut self,
    ) -> ComponentHooksRegisterer<'w, T> {
        if *APP_BUILT.read() {
            panic!(
                "AvilexComponentHooksError\n\
                Cannot register manual component hooks for {:?} after App::build() has been finalized!\n\
                All hook registrations must occur during the initial configuration phase.",
                std::any::type_name::<T>()
            );
        }

        let type_id = TypeId::of::<T>();
        for entry in self.archetypes_manager.iter_archetypes() {
            let arch = entry.value().lock();
            if arch.types.contains(&type_id) {
                panic!(
                    "AvilexComponentHooksError\n\
                    Cannot register manual component hooks for {:?} because an entity with this component type has already been spawned!\n\
                    Hook registrations must be established before data population begins.",
                    std::any::type_name::<T>()
                );
            }
        }

        self.component_hooks_registry.register_hooks::<T>()
    }

    /// Immediately spawns a new [`Entity`] with the given [`ComponentBundle`] and returns its handle.
    ///
    /// [`Entity`]: Entity
    /// [`ComponentBundle`]: crate::ecs::component::ComponentBundle
    pub fn spawn<C: ComponentBundle>(&mut self, component_bundle: C) -> Entity {
        let cmd = SpawnCommandCore {
            components: component_bundle,
        };
        cmd.apply(self)
    }

    /// Immediately spawns a batch of entities from `components_iter` and returns their [`Entity`]
    /// handles.
    ///
    /// The returned `Vec<Entity>` preserves the iteration order of `components_iter`.
    ///
    /// [`Entity`]: Entity
    pub fn spawn_batch<C, I>(&mut self, components_iter: I) -> Vec<Entity>
    where
        C: ComponentBundle,
        I: Iterator<Item = C> + Send + 'static,
    {
        let cmd = BatchSpawnCommandCore::<C, I> { components_iter };
        cmd.apply(self)
    }

    /// Immediately inserts the components in `component_bundle` onto `entity`.
    ///
    /// Any existing components of the same types on `entity` are overwritten.
    ///
    /// [`ComponentBundle`]: crate::ecs::component::ComponentBundle
    pub fn insert_components<B: ComponentBundle>(&mut self, entity: Entity, component_bundle: B) {
        let cmd = InsertComponentsCommand {
            entity,
            components: component_bundle,
        };
        cmd.apply(self);
    }

    /// Immediately removes all component types in `B` from `entity`.
    ///
    /// [`ComponentBundle`]: crate::ecs::component::ComponentBundle
    pub fn remove_components<B: ComponentBundle>(&mut self, entity: Entity) {
        let cmd = RemoveComponentsCommand::<B> {
            entity,
            _marker: PhantomData,
        };
        cmd.apply(self);
    }

    /// Immediately despawns `entity` and frees its [`Entity`] slot.
    ///
    /// If `entity` was already queued for despawn via [`Commands`], its queued handle is
    /// removed so it does not trigger a false handle-leak panic under Avilex's 32-bit
    /// generationless [`Entity`] rule.
    ///
    /// [`Entity`]: Entity
    /// [`Commands`]: Commands
    pub fn despawn(&mut self, entity: Entity) {
        if self.command_buffer.despawns.remove(&entity) {
            unsafe {
                (*ENTITY_HANDLE_COUNT_REGISTRY.get_ptr(entity.registry_index() as usize))
                    .decrement();
            }
        }
        let cmd = DespawnCommand { entity };
        cmd.apply(self);
    }
}

impl World {
    /// # Safety
    ///
    /// The caller must guarantee that:
    /// 1. This method is called exclusively from the **Main Thread** (the thread on which the non-send resource was originally registered and allocated).
    /// 2. No other active references (`&T` or `&mut T`) to this specific resource exist anywhere in the engine for the duration of the returned `NonSendResMut` lifetime.
    /// 3. Concurrent execution phases do not attempt to look up or access this resource type asynchronously from background worker pool threads.
    pub unsafe fn non_send_resource_mut_unsafe<'w, T: Resource>(&'w self) -> NonSendResMut<'w, T> {
        unsafe { self.get_non_send_resource_mut_unsafe::<T>() }.unwrap_or_else(|| {
            panic!(
                "NonSendResResourceMissingError\n\
                Name: {}",
                type_name::<T>()
            );
        })
    }

    /// # Safety
    ///
    /// The caller must guarantee that:
    /// 1. This method is called exclusively from the **Main Thread**.
    /// 2. No other active mutable or immutable references to this specific resource exist simultaneously within the current execution scope.
    /// 3. Access is strictly isolated from out-of-band parallel execution workers.
    pub unsafe fn get_non_send_resource_mut_unsafe<'w, T: Resource>(
        &'w self,
    ) -> Option<NonSendResMut<'w, T>> {
        let type_id = TypeId::of::<T>();
        let cell = self.non_send_resources.get(&type_id)?;
        let base_any = cell.get();
        let casted_mut = unsafe { (&mut *base_any).downcast_mut::<T>()? };
        Some(NonSendResMut { res: casted_mut })
    }
}
