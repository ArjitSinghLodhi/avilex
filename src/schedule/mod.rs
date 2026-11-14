use crate::{
    app::{App, dyn_traits::DynEq, plugin::Plugin},
    schedule::executors::multi_threaded::MultiThreadedExecutor,
    system::{System, SystemNode},
    world::World,
};

pub(crate) mod dyn_eq;
pub(crate) mod executors;
pub(crate) mod schedule_sorter;
mod schedules_list;
pub(crate) mod system_sorter;

pub use schedules_list::{First, Last, PostUpdate, PreUpdate, Startup, Update};

use std::{any::Any, fmt::Debug};

/// A [`Plugin`] that registers the default frame schedules ([`First`], [`PreUpdate`], [`Update`],
/// [`PostUpdate`], and [`Last`]) and configures their execution order.
///
/// [`Plugin`]: crate::app::plugin::Plugin
/// [`First`]: crate::app::schedule::First
/// [`PreUpdate`]: crate::app::schedule::PreUpdate
/// [`Update`]: crate::app::schedule::Update
/// [`PostUpdate`]: crate::app::schedule::PostUpdate
/// [`Last`]: crate::app::schedule::Last
pub struct DefaultSchedulesPlugin;

impl Plugin for DefaultSchedulesPlugin {
    fn build(&self, app: &mut App) {
        app.add_schedule(Schedule::new(First))
            .add_schedule(Schedule::new(PreUpdate))
            .add_schedule(Schedule::new(Update))
            .add_schedule(Schedule::new(PostUpdate))
            .add_schedule(Schedule::new(Last));

        app.configure_schedule_order(First, PreUpdate)
            .configure_schedule_order(PreUpdate, Update)
            .configure_schedule_order(Update, PostUpdate)
            .configure_schedule_order(PostUpdate, Last);
    }
}

/// A marker trait used to identify and configure a [`Schedule`].
///
/// Derive this using `#[derive(ScheduleLabel, Clone, PartialEq, Debug)]`.
///
/// [`Schedule`]: crate::app::schedule::Schedule
pub trait ScheduleLabel: Any + Send + Sync + DynEq + Debug + 'static {
    /// Creates the default [`SystemExecutor`] used when constructing a [`Schedule`] with this label.
    ///
    /// Defaults to [`MultiThreadedExecutor`].
    ///
    /// [`SystemExecutor`]: crate::app::schedule::SystemExecutor
    /// [`Schedule`]: crate::app::schedule::Schedule
    /// [`MultiThreadedExecutor`]: crate::app::schedule::MultiThreadedExecutor
    fn default_executor(&mut self) -> Box<dyn SystemExecutor> {
        Box::new(MultiThreadedExecutor::new())
    }

    /// Clones this label into a new boxed [`ScheduleLabel`].
    ///
    /// [`ScheduleLabel`]: Self
    fn dyn_clone(&self) -> Box<dyn ScheduleLabel>;
}

/// Container holding the ordered list of [`SystemNode`] entries belonging to a [`Schedule`].
///
/// [`SystemNode`]: crate::app::system::SystemNode
/// [`Schedule`]: crate::app::schedule::Schedule
pub struct SystemsSchedule {
    systems: Vec<SystemNode<(), ()>>,
}

impl SystemsSchedule {
    pub(crate) fn new() -> Self {
        Self {
            systems: Vec::new(),
        }
    }

    /// Returns a shared reference to the list of scheduled [`SystemNode`] entries.
    ///
    /// [`SystemNode`]: crate::app::system::SystemNode
    pub fn systems(&self) -> &Vec<SystemNode<(), ()>> {
        &self.systems
    }

    /// Returns a mutable reference to the list of scheduled [`SystemNode`] entries.
    ///
    /// [`SystemNode`]: crate::app::system::SystemNode
    pub fn systems_mut(&mut self) -> &mut Vec<SystemNode<(), ()>> {
        &mut self.systems
    }
}

/// Strategy trait defining how the systems inside a [`SystemsSchedule`] are initialized and executed.
///
/// [`SystemsSchedule`]: crate::app::schedule::SystemsSchedule
pub trait SystemExecutor: 'static {
    /// Prepares or partitions `schedule` before execution begins.
    fn init(&mut self, schedule: &mut SystemsSchedule);

    /// Executes the systems in `schedule` against `world`.
    fn run(&mut self, schedule: &mut SystemsSchedule, world: &mut World);
}

/// A labeled collection of systems paired with a [`SystemExecutor`] that runs them.
///
/// [`SystemExecutor`]: crate::app::schedule::SystemExecutor
pub struct Schedule {
    label: Box<dyn ScheduleLabel>,
    systems_schedule: SystemsSchedule,
    executor: Box<dyn SystemExecutor>,
}

impl Schedule {
    /// Creates a new [`Schedule`] identified by `label`, using [`ScheduleLabel::default_executor`].
    ///
    /// [`Schedule`]: Self
    /// [`ScheduleLabel::default_executor`]: crate::app::schedule::ScheduleLabel::default_executor
    pub fn new(label: impl ScheduleLabel) -> Self {
        Self::new_boxed(Box::new(label))
    }

    /// Creates a new [`Schedule`] from a boxed [`ScheduleLabel`], using its default executor.
    ///
    /// [`Schedule`]: Self
    /// [`ScheduleLabel`]: crate::app::schedule::ScheduleLabel
    pub fn new_boxed(mut label: Box<dyn ScheduleLabel>) -> Self {
        Self {
            executor: label.default_executor(),
            label,
            systems_schedule: SystemsSchedule::new(),
        }
    }

    /// Builder method that replaces the schedule's [`SystemExecutor`].
    ///
    /// [`SystemExecutor`]: crate::app::schedule::SystemExecutor
    pub fn with_executor(self, executor: impl SystemExecutor) -> Self {
        self.with_executor_boxed(Box::new(executor))
    }

    /// Builder method that replaces the schedule's [`SystemExecutor`] with a boxed instance.
    ///
    /// [`SystemExecutor`]: crate::app::schedule::SystemExecutor
    pub fn with_executor_boxed(mut self, executor: Box<dyn SystemExecutor>) -> Self {
        self.set_executor_boxed(executor);
        self
    }

    /// Builder method that replaces the schedule's [`ScheduleLabel`].
    ///
    /// [`ScheduleLabel`]: crate::app::schedule::ScheduleLabel
    pub fn with_label(self, label: impl ScheduleLabel) -> Self {
        self.with_label_boxed(Box::new(label))
    }

    /// Builder method that replaces the schedule's [`ScheduleLabel`] with a boxed instance.
    ///
    /// [`ScheduleLabel`]: crate::app::schedule::ScheduleLabel
    pub fn with_label_boxed(mut self, label: Box<dyn ScheduleLabel>) -> Self {
        self.set_label_boxed(label);
        self
    }

    /// Replaces the schedule's [`SystemExecutor`].
    ///
    /// [`SystemExecutor`]: crate::app::schedule::SystemExecutor
    pub fn set_executor(&mut self, executor: impl SystemExecutor) {
        self.set_executor_boxed(Box::new(executor));
    }

    /// Replaces the schedule's [`SystemExecutor`] with a boxed instance.
    ///
    /// [`SystemExecutor`]: crate::app::schedule::SystemExecutor
    pub fn set_executor_boxed(&mut self, executor: Box<dyn SystemExecutor>) {
        self.executor = executor;
    }

    /// Builder method that appends a [`System`] to this schedule.
    ///
    /// [`System`]: crate::app::system::System
    pub fn with_system(self, system: impl System<In = (), Out = ()> + 'static) -> Self {
        self.with_system_boxed(Box::new(system))
    }

    /// Builder method that appends a boxed [`System`] to this schedule.
    ///
    /// [`System`]: crate::app::system::System
    pub fn with_system_boxed(mut self, system: Box<dyn System<In = (), Out = ()>>) -> Self {
        self.add_system_boxed(system);
        self
    }

    /// Appends a [`System`] to this schedule.
    ///
    /// [`System`]: crate::app::system::System
    pub fn add_system(&mut self, system: impl System<In = (), Out = ()> + 'static) {
        self.systems_schedule.systems.push(SystemNode::new(system));
    }

    /// Appends a boxed [`System`] to this schedule.
    ///
    /// [`System`]: crate::app::system::System
    pub fn add_system_boxed(&mut self, system: Box<dyn System<In = (), Out = ()>>) {
        self.systems_schedule
            .systems
            .push(SystemNode::new_boxed(system));
    }

    /// Returns a shared reference to the underlying [`SystemsSchedule`].
    ///
    /// [`SystemsSchedule`]: crate::app::schedule::SystemsSchedule
    pub fn systems_schedule(&self) -> &SystemsSchedule {
        &self.systems_schedule
    }

    /// Returns a mutable reference to the underlying [`SystemsSchedule`].
    ///
    /// [`SystemsSchedule`]: crate::app::schedule::SystemsSchedule
    pub fn systems_schedule_mut(&mut self) -> &mut SystemsSchedule {
        &mut self.systems_schedule
    }

    /// Returns a shared reference to the schedule's [`ScheduleLabel`].
    ///
    /// [`ScheduleLabel`]: crate::app::schedule::ScheduleLabel
    pub fn label(&self) -> &dyn ScheduleLabel {
        &*self.label
    }

    /// Returns a mutable reference to the schedule's [`ScheduleLabel`].
    ///
    /// [`ScheduleLabel`]: crate::app::schedule::ScheduleLabel
    pub fn label_mut(&mut self) -> &mut dyn ScheduleLabel {
        &mut *self.label
    }

    /// Replaces the schedule's [`ScheduleLabel`].
    ///
    /// [`ScheduleLabel`]: crate::app::schedule::ScheduleLabel
    pub fn set_label(&mut self, label: impl ScheduleLabel) {
        self.label = Box::new(label);
    }

    /// Replaces the schedule's [`ScheduleLabel`] with a boxed instance.
    ///
    /// [`ScheduleLabel`]: crate::app::schedule::ScheduleLabel
    pub fn set_label_boxed(&mut self, label: Box<dyn ScheduleLabel>) {
        self.label = label;
    }

    /// Initializes the schedule's [`SystemExecutor`] with the current [`SystemsSchedule`].
    ///
    /// [`SystemExecutor`]: crate::app::schedule::SystemExecutor
    /// [`SystemsSchedule`]: crate::app::schedule::SystemsSchedule
    pub fn init_executor(&mut self) {
        self.executor.init(&mut self.systems_schedule);
    }

    /// Runs all systems in this schedule against `world` using the configured [`SystemExecutor`].
    ///
    /// [`SystemExecutor`]: crate::app::schedule::SystemExecutor
    pub fn run(&mut self, world: &mut World) {
        self.executor.run(&mut self.systems_schedule, world);
    }
}

pub(crate) struct ScheduleConstraint {
    pub(crate) before: Box<dyn ScheduleLabel>,
    pub(crate) after: Box<dyn ScheduleLabel>,
}
