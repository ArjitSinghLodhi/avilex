pub(crate) mod plugin;
use parking_lot::RwLock;
pub use plugin::Plugin;
use rustc_hash::FxHashSet;

use crate::{
    app_impl::plugin::PluginGroup,
    resources::Resource,
    schedule::{
        DefaultSchedulesPlugin, Schedule, ScheduleConstraint, ScheduleLabel,
        schedule_sorter::sort_schedules, schedules_list::Startup,
    },
    states::{States, setup_states_schedules_and_systems},
    system::{
        system_configs_traits::IntoSystemConfigs,
        system_error::{SystemErrorHandler, SystemErrorHandlerFn},
        system_set_traits::IntoSystemSetConfigs,
    },
    world::World,
};
use std::{
    any::TypeId,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::events::{Event, EventBuffer, register_event};
use std::any::type_name;

use crate::{ecs::component::Component, reactivity::removed::register_removal_tracking_buffers};

static APP_INITIALIZED: AtomicBool = AtomicBool::new(false);

pub(crate) static APP_BUILT: RwLock<bool> = RwLock::new(false);

pub(crate) struct ConfigurationContext {
    building_plugins: bool,
    schedules_processed: bool,
    systems_processed: bool,
    built: bool,
    ran_startup: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AvilexAppConfigurationError {
    SchedulesNotInitialized,
    SystemsNotConfigured,
    CriticalEngineBuildMismatch,
    StartupNotProcessed,
    AppAlreadyBuilt,
}

impl ConfigurationContext {
    fn new() -> Self {
        Self {
            building_plugins: false,
            schedules_processed: false,
            systems_processed: false,
            built: false,
            ran_startup: false,
        }
    }

    fn is_building_plugins(&self) -> bool {
        self.building_plugins
    }

    fn schedules_processed(&self) {
        if !self.schedules_processed {
            panic!("{:?}", AvilexAppConfigurationError::SchedulesNotInitialized);
        }
    }

    fn systems_processed(&self) {
        if !self.systems_processed {
            panic!("{:?}", AvilexAppConfigurationError::SystemsNotConfigured);
        }
    }

    fn built(&self) {
        self.schedules_processed();
        self.systems_processed();
        if !self.built {
            panic!(
                "{:?}",
                AvilexAppConfigurationError::CriticalEngineBuildMismatch
            );
        }
    }

    fn ran_startup(&self) {
        if !self.ran_startup {
            panic!("{:?}", AvilexAppConfigurationError::StartupNotProcessed);
        }
    }

    fn not_ready(&self) {
        if self.built || self.ran_startup || *APP_BUILT.read() {
            panic!("{:?}", AvilexAppConfigurationError::AppAlreadyBuilt);
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

type RunnerFn = Box<dyn FnOnce(&mut App) + 'static>;

/// The primary entry point and builder for configuring and executing an Avilex ECS application.
///
/// [`App`] owns the underlying [`World`], manages registered [`Plugin`]s, organizes [`System`]s
/// into labeled [`Schedule`]s and [`SystemSet`]s, and executes the application lifecycle via its
/// configured runner.
///
/// # Lifecycle & Execution Flow
/// 1. **Construction:** Start with [`App::new`] (which registers [`DefaultSchedulesPlugin`] and the
///    [`Startup`] schedule) or [`App::empty`] for a completely blank slate.
/// 2. **Configuration:** Register systems ([`App::add_systems`]), plugins ([`App::add_plugins`]),
///    resources ([`App::insert_resource`]), events ([`App::init_event`]), states
///    ([`App::init_state`]), and schedule/set ordering constraints.
/// 3. **Finalization (`build`):** Calling [`App::build`] resolves schedule and system ordering,
///    initializes executors, and auto-registers reactive component trackers.
/// 4. **Startup (`run_startup`):** Calling [`App::run_startup`] executes the [`Startup`] schedule
///    once. When driving the app manually (instead of using [`App::run`] or inside a runner called by [`App::run`]), you **must** call
///    [`App::run_startup`] after [`App::build`] before calling [`App::update`], as
///    [`App::update`] does not run startup systems.
/// 5. **Execution (`update` / `run`):** Calling [`App::update`] runs all frame schedules once in
///    their resolved topological order and flushes double-buffered event queues and clear reactivity trackers.
///    Calling [`App::run`] delegates to the configured runner (which by default runs a single
///    cycle of `build` -> `run_startup` -> `update`, unless customized via [`App::set_runner`]).
///
/// # Example
/// ```rust
#[doc = include_str!("../../examples/01_hello_world.rs")]
/// ```
///
/// [`World`]: crate::ecs::world::World
/// [`Plugin`]: crate::app::plugin::Plugin
/// [`System`]: crate::app::system::System
/// [`Schedule`]: crate::app::schedule::Schedule
/// [`SystemSet`]: crate::app::system::system_set::SystemSet
/// [`App::new`]: Self::new
/// [`DefaultSchedulesPlugin`]: crate::app::schedule::DefaultSchedulesPlugin
/// [`Startup`]: crate::app::schedule::Startup
/// [`App::empty`]: Self::empty
/// [`App::add_systems`]: Self::add_systems
/// [`App::add_plugins`]: Self::add_plugins
/// [`App::insert_resource`]: Self::insert_resource
/// [`App::init_event`]: Self::init_event
/// [`App::init_state`]: Self::init_state
/// [`App::build`]: Self::build
/// [`App::run_startup`]: Self::run_startup
/// [`App::update`]: Self::update
/// [`App::run`]: Self::run
/// [`App::set_runner`]: Self::set_runner
pub struct App {
    world: World,
    registered_plugins: FxHashSet<TypeId>,
    startup_schedule: Schedule,
    schedules: Vec<Schedule>,
    pub(crate) schedule_order_constraints: Vec<ScheduleConstraint>,
    runner_fn: RunnerFn,
    configuration_context: ConfigurationContext,
}

impl App {
    /// Creates a new [`App`] pre-configured with [`DefaultSchedulesPlugin`] and the [`Startup`]
    /// schedule.
    ///
    /// # Panics
    /// Panics if an [`App`] instance has already been created in the current process.
    ///
    /// [`App`]: crate::app::App
    /// [`DefaultSchedulesPlugin`]: crate::app::schedule::DefaultSchedulesPlugin
    /// [`Startup`]: crate::app::schedule::Startup
    pub fn new() -> Self {
        let mut app = Self::empty();
        PluginGroup::build(DefaultSchedulesPlugin).finish(&mut app);
        app
    }

    /// Creates a blank [`App`] with no default frame schedules, containing only [`Startup`].
    ///
    /// # Panics
    /// Panics if an [`App`] instance has already been created in the current process.
    ///
    /// [`App`]: crate::app::App
    /// [`Startup`]: crate::app::schedule::Startup
    pub fn empty() -> Self {
        if APP_INITIALIZED.swap(true, Ordering::SeqCst) {
            panic!(
                "MultipleAppInstancesError\n\
                Ensure you only instantiate exactly one App::new() across your entire binary runtime."
            );
        }

        Self {
            world: World::new(),
            registered_plugins: FxHashSet::default(),
            startup_schedule: Schedule::new(Startup),
            schedules: Vec::new(),
            schedule_order_constraints: Vec::new(),
            runner_fn: Box::new(runner_once),
            configuration_context: ConfigurationContext::new(),
        }
    }

    /// Returns `true` if a frame [`Schedule`] with the given `label` has been added to this [`App`].
    ///
    /// [`Schedule`]: crate::app::schedule::Schedule
    /// [`App`]: crate::app::App
    pub fn contains_schedule(&self, label: &impl ScheduleLabel) -> bool {
        self.schedules
            .iter()
            .any(|schedule| schedule.label().dyn_eq(label))
    }

    /// Initializes a [`States`] machine of type `T` using `T::default()` and registers its
    /// [`State<T>`] and [`NextState<T>`] resources along with transition systems.
    ///
    /// [`States`]: crate::app::states::States
    /// [`State<T>`]: crate::app::states::State
    /// [`NextState<T>`]: crate::app::states::NextState
    pub fn init_state<T: States + Default>(&mut self) -> &mut Self {
        self.insert_state(T::default())
    }

    /// Inserts a [`States`] machine initialized to `state` and registers its [`State<T>`] and
    /// [`NextState<T>`] resources along with transition schedules and systems.
    ///
    /// [`States`]: crate::app::states::States
    /// [`State<T>`]: crate::app::states::State
    /// [`NextState<T>`]: crate::app::states::NextState
    pub fn insert_state<T: States>(&mut self, state: T) -> &mut Self {
        self.configuration().not_ready();
        setup_states_schedules_and_systems(self, state);
        self
    }

    /// Inserts a new schedule under the provided label, overwriting any existing schedule with the same label.
    pub fn insert_schedule(&mut self, schedule: Schedule) -> &mut Self {
        self.configuration().not_ready();

        if schedule.label().dyn_eq(&Startup) {
            self.startup_schedule = schedule;
        } else {
            match self
                .schedules
                .iter()
                .position(|s| s.label().dyn_eq(schedule.label()))
            {
                Some(index) => self.schedules[index] = schedule,
                None => self.schedules.push(schedule),
            }
        }
        self
    }

    /// Initializes an empty `schedule` under the provided `label`, if it does not exist.
    ///
    /// See [`insert_schedule`] to insert an existing schedule.
    ///
    /// [`insert_schedule`]: crate::app::App::insert_schedule
    pub fn init_schedule(&mut self, label: impl ScheduleLabel) -> &mut Self {
        if !label.dyn_eq(&Startup) && !self.schedules.iter().any(|s| s.label().dyn_eq(&label)) {
            self.schedules.push(Schedule::new(label));
        }

        self
    }

    /// Returns a reference to the [`Schedule`] with the provided `label` if it exists.
    ///
    /// [`Schedule`]: crate::app::schedule::Schedule
    pub fn get_schedule(&self, label: impl ScheduleLabel) -> Option<&Schedule> {
        if label.dyn_eq(&Startup) {
            Some(&self.startup_schedule)
        } else {
            self.schedules.iter().find(|s| s.label().dyn_eq(&label))
        }
    }

    /// Returns a mutable reference to the [`Schedule`] with the provided `label` if it exists.
    ///
    /// [`Schedule`]: crate::app::schedule::Schedule
    pub fn get_schedule_mut(&mut self, label: impl ScheduleLabel) -> Option<&mut Schedule> {
        if label.dyn_eq(&Startup) {
            Some(&mut self.startup_schedule)
        } else {
            self.schedules.iter_mut().find(|s| s.label().dyn_eq(&label))
        }
    }

    /// Runs function `f` with the [`Schedule`] associated with `label`.
    ///
    /// **Note:** This will create the schedule if it does not already exist.
    ///
    /// [`Schedule`]: crate::app::schedule::Schedule
    pub fn edit_schedule(
        &mut self,
        label: impl ScheduleLabel,
        f: impl FnOnce(&mut Schedule),
    ) -> &mut Self {
        if label.dyn_eq(&Startup) {
            f(&mut self.startup_schedule);
        } else {
            match self.schedules.iter().position(|s| s.label().dyn_eq(&label)) {
                Some(index) => f(&mut self.schedules[index]),
                None => {
                    let mut schedule = Schedule::new(label);
                    f(&mut schedule);
                    self.schedules.push(schedule);
                }
            }
        }

        self
    }

    /// Configures one or more system sets in `schedule`.
    pub fn configure_sets(
        &mut self,
        schedule: impl ScheduleLabel,
        sets: impl IntoSystemSetConfigs,
    ) -> &mut Self {
        self.configuration().not_ready();
        self.edit_schedule(schedule, |schedule| {
            schedule.configure_sets(sets);
        })
    }

    /// Adds a topological ordering constraint requiring schedule `before` to execute prior to
    /// schedule `after` each frame.
    pub fn configure_schedule_order(
        &mut self,
        before: impl ScheduleLabel,
        after: impl ScheduleLabel,
    ) -> &mut Self {
        self.configuration().not_ready();
        self.schedule_order_constraints.push(ScheduleConstraint {
            before: Box::new(before),
            after: Box::new(after),
        });
        self
    }

    /// Registers one or more systems into `schedule`.
    pub fn add_systems<M>(
        &mut self,
        schedule: impl ScheduleLabel,
        systems: impl IntoSystemConfigs<M>,
    ) -> &mut Self {
        self.configuration().not_ready();
        self.edit_schedule(schedule, |schedule| {
            schedule.add_systems(systems);
        })
    }

    /// Adds a [`PluginGroup`] and immediately builds them on this [`App`].
    ///
    /// [`PluginGroup`]: crate::app::plugin::PluginGroup
    /// [`App`]: crate::app::App
    pub fn add_plugins(&mut self, plugins: impl PluginGroup) -> &mut Self {
        self.configuration().not_ready();
        self.configuration().building_plugins = true;
        plugins.build().finish(self);
        self.configuration().building_plugins = false;
        self
    }

    /// Returns `true` if a unique [`Plugin`] of type `T` has already been built on this [`App`].
    ///
    /// [`Plugin`]: crate::app::plugin::Plugin
    /// [`App`]: crate::app::App
    pub fn is_plugin_built<T: Plugin>(&self) -> bool {
        self.registered_plugins.contains(&TypeId::of::<T>())
    }

    /// Returns `true` if the [`App`] is currently building plugins through [`App::add_plugins`].
    ///
    /// [`App`]: App
    /// [`App::add_plugins`]: crate::app::App::add_plugins
    pub fn is_building_plugins(&self) -> bool {
        self.configuration_ref().is_building_plugins()
    }

    /// Returns `true` if [`App::build`] has completed.
    ///
    /// [`App::build`]: Self::build
    pub fn is_built(&self) -> bool {
        self.configuration_ref().built
    }

    /// Returns `true` if [`App::run_startup`] has already executed.
    ///
    /// [`App::run_startup`]: Self::run_startup
    pub fn ran_startup(&self) -> bool {
        self.configuration_ref().ran_startup
    }

    /// Registers an [`Event`] type `T`, inserting its double-buffered resource
    /// so systems can use [`EventWriter<T>`] and [`EventReader<T>`].
    ///
    /// # Panics
    /// Panics if event type `T` has already been initialized on this [`App`].
    ///
    /// [`Event`]: crate::ecs::events::Event
    /// [`EventWriter<T>`]: crate::ecs::events::EventWriter
    /// [`EventReader<T>`]: crate::ecs::events::EventReader
    /// [`App`]: crate::app::App
    pub fn init_event<T: Event>(&mut self) -> &mut Self {
        self.configuration().not_ready();
        if self.world.contains_resource::<EventBuffer<T>>() {
            panic!(
                "EventAlreadyInitializedError\n\
                Name: {}",
                type_name::<T>()
            );
        }
        self.world.insert_resource(EventBuffer::<T>::new());
        register_event::<T>();
        self
    }

    /// Finalizes the [`App`] by resolving schedule ordering (and draining consumed schedule
    /// ordering constraints), initializing all registered [`Schedule`]s, and registering
    /// component removal tracking buffers.
    ///
    /// See [`Schedule::initialize`] for details on how each schedule's systems, [`SystemSet`]
    /// constraints, condition guard nodes, and rebuild metadata are processed.
    ///
    /// # Panics
    /// Panics if called while building a [`Plugin`] inside [`App::add_plugins`], or if
    /// [`App::build`] has already been called.
    ///
    /// [`App`]: crate::app::App
    /// [`Schedule`]: crate::app::schedule::Schedule
    /// [`Schedule::initialize`]: crate::app::schedule::Schedule::initialize
    /// [`SystemSet`]: crate::app::system::system_set::SystemSet
    /// [`Plugin`]: crate::app::plugin::Plugin
    /// [`App::add_plugins`]: Self::add_plugins
    /// [`App::build`]: Self::build
    pub fn build(&mut self) -> &mut Self {
        if self.is_building_plugins() {
            panic!(
                "AppLifecycleError\n\
                Details: App::build() was called while building plugins"
            );
        }
        self.configuration().not_ready();
        self.build_everything();
        *APP_BUILT.write() = true;
        self.configuration().built = true;
        self
    }

    /// Executes the [`Startup`] schedule once.
    ///
    /// Must be called after [`App::build`] and before [`App::update`] when driving the [`App`]
    /// manually.
    ///
    /// # Panics
    /// Panics if called from inside [`Plugin::build`], before [`App::build`], or if
    /// [`App::run_startup`] has already been called.
    ///
    /// [`Startup`]: crate::app::schedule::Startup
    /// [`App::build`]: Self::build
    /// [`App::update`]: Self::update
    /// [`App`]: crate::app::App
    /// [`Plugin::build`]: crate::app::plugin::Plugin::build
    /// [`App::run_startup`]: Self::run_startup
    pub fn run_startup(&mut self) -> &mut Self {
        if self.is_building_plugins() {
            panic!(
                "AppLifecycleError\n\
                Details: App::run_startup() was called while building plugins"
            );
        }
        self.configuration().built();
        if self.configuration().ran_startup {
            panic!(
                "AppLifecycleError\n\
                Details: Startup schedule has already executed"
            );
        }
        self.startup_schedule.run(&mut self.world);
        self.configuration().ran_startup = true;
        self
    }

    /// Advances the [`App`] by a single frame: synchronizes frame state, runs all scheduled frame
    /// [`Schedule`]s in topological order, flushes queued commands and despawns, and swaps
    /// double-buffered event and reactivity queues.
    ///
    /// # Panics
    /// Panics if called from inside [`Plugin::build`], before [`App::build`], or before
    /// [`App::run_startup`].
    ///
    /// [`App`]: crate::app::App
    /// [`Schedule`]: crate::app::schedule::Schedule
    /// [`Plugin::build`]: crate::app::plugin::Plugin::build
    /// [`App::build`]: Self::build
    /// [`App::run_startup`]: Self::run_startup
    pub fn update(&mut self) {
        if self.is_building_plugins() {
            panic!(
                "AppLifecycleError\n\
                Details: App::update() was called while building plugins"
            );
        }
        self.configuration().built();
        self.configuration().ran_startup();
        self.world_mut().start_frame_sync();
        for schedule in self.schedules.iter_mut() {
            schedule.run(&mut self.world);
        }
        self.world.apply_queue_commands();
        self.world.apply_despawns();
        self.world_mut().end_of_frame_sync();
    }

    /// Consumes the configured runner callback and executes the [`App`].
    ///
    /// By default, runs a single cycle of [`App::build`], [`App::run_startup`], and
    /// [`App::update`], unless a custom runner was installed via [`App::set_runner`].
    ///
    /// # Panics
    /// Panics if called from inside [`Plugin::build`].
    ///
    /// [`App`]: crate::app::App
    /// [`App::build`]: Self::build
    /// [`App::run_startup`]: Self::run_startup
    /// [`App::update`]: Self::update
    /// [`App::set_runner`]: Self::set_runner
    /// [`Plugin::build`]: crate::app::plugin::Plugin::build
    pub fn run(&mut self) {
        if self.is_building_plugins() {
            panic!(
                "AppLifecycleError\n\
                Details: App::run() was called while building plugins"
            );
        }
        let function = std::mem::replace(&mut self.runner_fn, Box::new(runner_once));
        function(self);
    }

    /// Replaces the runner callback invoked by [`App::run`].
    ///
    /// [`App::run`]: Self::run
    pub fn set_runner(&mut self, function: impl FnOnce(&mut App) + 'static) -> &mut Self {
        self.runner_fn = Box::new(function);
        self
    }

    /// Sets the global [`SystemErrorHandlerFn`] callback invoked whenever a system fails or is
    /// skipped during execution.
    ///
    /// Inserts a [`SystemErrorHandler`] resource into the [`World`], overriding
    /// [`default_system_error_handler`].
    ///
    /// [`SystemErrorHandlerFn`]: crate::app::system::system_error::SystemErrorHandlerFn
    /// [`SystemErrorHandler`]: crate::app::system::system_error::SystemErrorHandler
    /// [`World`]: crate::ecs::world::World
    /// [`default_system_error_handler`]: crate::app::system::system_error::default_system_error_handler
    pub fn set_system_error_handler(&mut self, handler: SystemErrorHandlerFn) -> &mut Self {
        self.insert_resource(SystemErrorHandler(handler))
    }

    /// Initializes a thread-safe [`Resource`] of type `T` using `T::default()` and inserts it into
    /// the [`World`].
    ///
    /// [`Resource`]: crate::ecs::resources::Resource
    /// [`World`]: crate::ecs::world::World
    pub fn init_resource<T: Resource + Send + Sync + Default>(&mut self) -> &mut Self {
        self.insert_resource(T::default())
    }

    /// Inserts a thread-safe [`Resource`] of type `T` into the [`World`], overwriting any existing
    /// value of the same type.
    ///
    /// [`Resource`]: crate::ecs::resources::Resource
    /// [`World`]: crate::ecs::world::World
    pub fn insert_resource<T: Resource + Send + Sync>(&mut self, resource: T) -> &mut Self {
        self.world.insert_resource(resource);
        self
    }

    /// Removes the thread-safe [`Resource`] of type `T` from the [`World`], if present.
    ///
    /// [`Resource`]: crate::ecs::resources::Resource
    /// [`World`]: crate::ecs::world::World
    pub fn remove_resource<T: Resource + Send + Sync>(&mut self) -> &mut Self {
        self.world.remove_resource::<T>();
        self
    }

    /// Initializes a main-thread (`NonSend`) [`Resource`] of type `T` using `T::default()` and
    /// inserts it into the [`World`].
    ///
    /// [`Resource`]: crate::ecs::resources::Resource
    /// [`World`]: crate::ecs::world::World
    pub fn init_non_send_resource<T: Resource + Default>(&mut self) -> &mut Self {
        self.insert_non_send_resource(T::default())
    }

    /// Inserts a main-thread (`NonSend`) [`Resource`] of type `T` into the [`World`], overwriting
    /// any existing value of the same type.
    ///
    /// [`Resource`]: crate::ecs::resources::Resource
    /// [`World`]: crate::ecs::world::World
    pub fn insert_non_send_resource<T: Resource>(&mut self, resource: T) -> &mut Self {
        self.world.insert_non_send_resource(resource);
        self
    }

    /// Removes the main-thread (`NonSend`) [`Resource`] of type `T` from the [`World`], if
    /// present.
    ///
    /// [`Resource`]: crate::ecs::resources::Resource
    /// [`World`]: crate::ecs::world::World
    pub fn remove_non_send_resource<T: Resource>(&mut self) -> &mut Self {
        self.world.remove_non_send_resource::<T>();
        self
    }

    /// Explicitly enables double-buffered change tracking for component `T` on the underlying
    /// [`World`].
    ///
    /// Useful when change tracking is needed dynamically without a scheduled [`Changed<T>`] or
    /// [`ChangedRefMut<T>`] system parameter. See [`reactivity`] for details.
    ///
    /// [`World`]: crate::ecs::world::World
    /// [`Changed<T>`]: crate::ecs::reactivity::changed::Changed
    /// [`ChangedRefMut<T>`]: crate::ecs::reactivity::changed::ChangedRefMut
    /// [`reactivity`]: crate::ecs::reactivity
    pub fn register_changed_trackers<T: Component>(&mut self) -> &mut Self {
        self.world().register_changed_trackers::<T>();
        self
    }

    /// Explicitly enables double-buffered addition tracking for component `T` on the underlying
    /// [`World`].
    ///
    /// Useful when addition tracking is needed dynamically without a scheduled [`Added<T>`] or
    /// [`AddedTracker<T>`] system parameter. See [`reactivity`] for details.
    ///
    /// [`World`]: crate::ecs::world::World
    /// [`Added<T>`]: crate::ecs::reactivity::added::Added
    /// [`AddedTracker<T>`]: crate::ecs::reactivity::added::AddedTracker
    /// [`reactivity`]: crate::ecs::reactivity
    pub fn register_added_trackers<T: Component>(&mut self) -> &mut Self {
        self.world().register_added_trackers::<T>();
        self
    }

    /// Explicitly enables double-buffered removal tracking for component `T` on the underlying
    /// [`World`].
    ///
    /// Useful when removal tracking is needed dynamically without a scheduled
    /// [`RemovedComponents<T>`] system parameter. See [`reactivity`] for details.
    ///
    /// [`World`]: crate::ecs::world::World
    /// [`RemovedComponents<T>`]: crate::ecs::reactivity::removed::RemovedComponents
    /// [`reactivity`]: crate::ecs::reactivity
    pub fn register_removal_trackers<T: Component>(&mut self) -> &mut Self {
        self.world().register_removal_trackers::<T>();
        self
    }

    fn configuration(&mut self) -> &mut ConfigurationContext {
        &mut self.configuration_context
    }
    fn configuration_ref(&self) -> &ConfigurationContext {
        &self.configuration_context
    }

    /// Returns a shared reference to the underlying [`World`].
    ///
    /// [`World`]: crate::ecs::world::World
    pub fn world(&self) -> &World {
        &self.world
    }

    /// Returns a mutable reference to the underlying [`World`].
    ///
    /// [`World`]: crate::ecs::world::World
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }
}

impl App {
    fn build_everything(&mut self) {
        self.configure_schedules();
        self.configure_systems();
        register_removal_tracking_buffers(self.world_mut());
        self.schedule_order_constraints.clear();
    }

    fn configure_systems(&mut self) {
        self.startup_schedule.initialize();
        self.schedules
            .iter_mut()
            .for_each(|schedule| schedule.initialize());
        self.configuration().systems_processed = true;
    }

    fn configure_schedules(&mut self) {
        self.schedules = sort_schedules(
            std::mem::take(&mut self.schedules),
            &self.schedule_order_constraints,
        );
        self.configuration().schedules_processed = true;
    }
}

fn runner_once(app: &mut App) {
    app.build();
    app.run_startup();
    app.update();
}
