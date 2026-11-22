use crate::{
    app::{App, dyn_traits::DynEq, plugin::Plugin, system::SystemExt},
    schedule::{
        executors::{SystemExecutor, multi_threaded::MultiThreadedExecutor},
        schedules_list::{First, Last, PostUpdate, PreUpdate, Update},
        system_sorter::sort_schedule_systems,
    },
    system::{
        SystemIdentity, SystemNode,
        system_configs_traits::IntoSystemConfigs,
        system_set::{SetRegistration, SystemSet, preprocess_system_sets},
        system_set_traits::IntoSystemSetConfigs,
        system_traits::SystemOrderings,
    },
    world::World,
};

pub(crate) mod dyn_eq;
pub(crate) mod executors;
pub(crate) mod schedule_sorter;
pub(crate) mod schedules_list;
pub(crate) mod system_sorter;

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
        app.insert_schedule(Schedule::new(First))
            .insert_schedule(Schedule::new(PreUpdate))
            .insert_schedule(Schedule::new(Update))
            .insert_schedule(Schedule::new(PostUpdate))
            .insert_schedule(Schedule::new(Last));

        app.configure_schedule_order(First, PreUpdate)
            .configure_schedule_order(PreUpdate, Update)
            .configure_schedule_order(Update, PostUpdate)
            .configure_schedule_order(PostUpdate, Last);
    }
}

/// A trait used to identify and configure a [`Schedule`].
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
    fn default_executor(&self) -> Box<dyn SystemExecutor> {
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

/// A labeled collection of systems paired with a [`SystemExecutor`] that runs them.
///
/// [`SystemExecutor`]: crate::app::schedule::SystemExecutor
pub struct Schedule {
    label: Box<dyn ScheduleLabel>,
    systems_schedule: SystemsSchedule,
    executor: Box<dyn SystemExecutor>,
    set_registrations: Vec<SetRegistration>,
    dirty: bool,
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
    pub fn new_boxed(label: Box<dyn ScheduleLabel>) -> Self {
        Self {
            executor: label.default_executor(),
            label,
            systems_schedule: SystemsSchedule::new(),
            set_registrations: Vec::new(),
            dirty: false,
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

    /// Marks this schedule as needing to be rebuilt before its next run.
    pub fn set_dirty(&mut self) -> &mut Self {
        self.dirty = true;
        self
    }

    /// Appends one or more systems to this schedule.
    pub fn add_systems<M>(&mut self, systems: impl IntoSystemConfigs<M>) -> &mut Self {
        self.set_dirty();
        let configs = systems.into_configs();
        self.systems_schedule
            .systems
            .extend(configs.systems.into_iter().map(SystemNode::new_boxed));
        self
    }

    /// Builder method that appends one or more systems to this schedule.
    pub fn with_systems<M>(mut self, systems: impl IntoSystemConfigs<M>) -> Self {
        self.add_systems(systems);
        self
    }

    /// Configures one or more system sets in this schedule.
    pub fn configure_sets(&mut self, sets: impl IntoSystemSetConfigs) -> &mut Self {
        self.set_dirty();
        for config in sets.into_configs() {
            let registration = SetRegistration {
                set: config.set,
                run_after_sets: config.run_after_sets,
                run_before_sets: config.run_before_sets,
                run_after_systems: config.run_after_systems,
                run_before_systems: config.run_before_systems,
                member_of: config.member_of,
                condition_gates: config.condition_gates,
            };
            self.set_registrations.push(registration);
        }

        self
    }

    /// Builder method that configures one or more system sets in this schedule.
    pub fn with_configured_sets(mut self, sets: impl IntoSystemSetConfigs) -> Self {
        self.configure_sets(sets);
        self
    }

    /// Builds and initializes this schedule's systems and [`SystemSet`]s if dirty.
    ///
    /// This resolves [`SystemSet`] constraints, sorts systems into their execution order,
    /// initializes the configured [`SystemExecutor`], and extracts the condition gates from
    /// registered [`SystemSet`]s into guard nodes so they are not duplicated on subsequent
    /// rebuilds. If a guard node already exists for a [`SystemSet`], any newly added
    /// conditions are pushed onto that existing guard node instead of creating another one.
    ///
    /// **Note:** All [`SystemSet`] registrations, system ordering metadata, and previously
    /// generated condition guard nodes remain intact across rebuilds so newly added systems
    /// or new set conditions are properly validated and ordered relative to already-initialized
    /// systems.
    ///
    /// [`SystemExecutor`]: crate::app::schedule::SystemExecutor
    /// [`SystemSet`]: crate::app::system::system_set::SystemSet
    pub fn initialize(&mut self) {
        if !self.dirty {
            return;
        }

        deduplicate_orderings(
            &mut self.systems_schedule.systems,
            &mut self.set_registrations,
        );

        let mut generated_guards = Vec::new();
        preprocess_system_sets(
            &mut self.systems_schedule.systems,
            &mut self.set_registrations,
            &mut generated_guards,
        );
        self.systems_schedule.systems.extend(generated_guards);

        deduplicate_orderings(
            &mut self.systems_schedule.systems,
            &mut self.set_registrations,
        );

        sort_schedule_systems(&mut self.systems_schedule.systems);

        self.executor.init(&mut self.systems_schedule);

        self.dirty = false;
    }

    /// Builds the schedule if needed and executes all of its systems on `world` using the configured [`SystemExecutor`].
    ///
    /// [`SystemExecutor`]: crate::app::schedule::SystemExecutor
    pub fn run(&mut self, world: &mut World) {
        self.initialize();
        self.executor.run(&mut self.systems_schedule, world);
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
}

pub(crate) struct ScheduleConstraint {
    pub(crate) before: Box<dyn ScheduleLabel>,
    pub(crate) after: Box<dyn ScheduleLabel>,
}

fn dedup_sets(sets: &mut Vec<Box<dyn SystemSet>>) {
    let mut i = 0;
    while i < sets.len() {
        if sets[..i]
            .iter()
            .any(|existing| existing.dyn_eq(sets[i].as_ref()))
        {
            sets.remove(i);
        } else {
            i += 1;
        }
    }
}

fn dedup_identities(ids: &mut Vec<Box<dyn SystemIdentity>>) {
    let mut i = 0;
    while i < ids.len() {
        if ids[..i]
            .iter()
            .any(|existing| existing.dyn_system_eq(ids[i].as_ref()))
        {
            ids.remove(i);
        } else {
            i += 1;
        }
    }
}

pub(crate) fn deduplicate_orderings(
    schedule_systems: &mut [SystemNode<(), ()>],
    registrations: &mut Vec<SetRegistration>,
) {
    let mut i = 0;
    while i < registrations.len() {
        if let Some(existing_idx) = registrations[..i]
            .iter()
            .position(|existing| existing.set.dyn_eq(registrations[i].set.as_ref()))
        {
            let dup = registrations.remove(i);
            let target = &mut registrations[existing_idx];
            target.run_after_sets.extend(dup.run_after_sets);
            target.run_before_sets.extend(dup.run_before_sets);
            target.run_after_systems.extend(dup.run_after_systems);
            target.run_before_systems.extend(dup.run_before_systems);
            target.member_of.extend(dup.member_of);
            target.condition_gates.extend(dup.condition_gates);
        } else {
            i += 1;
        }
    }

    for reg in registrations.iter_mut() {
        dedup_sets(&mut reg.run_after_sets);
        dedup_sets(&mut reg.run_before_sets);
        dedup_sets(&mut reg.member_of);
        dedup_identities(&mut reg.run_after_systems);
        dedup_identities(&mut reg.run_before_systems);
    }

    for node in schedule_systems.iter_mut() {
        if let Some(orderings) = node.system_mut().get_data_mut::<SystemOrderings>() {
            dedup_sets(&mut orderings.run_after_sets);
            dedup_sets(&mut orderings.run_before_sets);
            dedup_sets(&mut orderings.member_of);
            dedup_identities(&mut orderings.run_after_systems);
            dedup_identities(&mut orderings.run_before_systems);
        }
    }
}

/// Converts a [`SystemExecutor`] instance or factory function into a boxed [`SystemExecutor`]
/// for a [`ScheduleLabel`].
///
/// This trait is primarily used by the `#[derive(ScheduleLabel)]` macro when the
/// `#[schedule_label(default_executor = ...)]` attribute is specified.
///
/// # Supported Inputs
///
/// - Any concrete [`SystemExecutor`] value directly (such as unit structs like `StartupExecutor`
///   or pre-configured executor instances).
/// - Any callable (`FnOnce`) taking `()`, `&L` (the concrete label), or `&dyn ScheduleLabel`
///   (a trait-object reference to the label), returning either a concrete `E: SystemExecutor`
///   (automatically boxed) or an already-boxed `Box<dyn SystemExecutor>`.
///
/// [`SystemExecutor`]: crate::app::schedule::SystemExecutor
/// [`ScheduleLabel`]: crate::app::schedule::ScheduleLabel
pub trait IntoDefaultExecutor<L: ScheduleLabel + ?Sized, Marker: ?Sized> {
    /// Converts `self` (or invokes it with `label`) into a boxed [`SystemExecutor`].
    ///
    /// [`SystemExecutor`]: crate::app::schedule::SystemExecutor
    fn into_executor(self, label: &L) -> Box<dyn SystemExecutor>;
}

impl<L: ScheduleLabel + ?Sized, E: SystemExecutor> IntoDefaultExecutor<L, dyn SystemExecutor>
    for E
{
    fn into_executor(self, _label: &L) -> Box<dyn SystemExecutor> {
        Box::new(self)
    }
}

impl<L: ScheduleLabel + ?Sized, F: FnOnce() -> E, E: SystemExecutor> IntoDefaultExecutor<L, ()>
    for F
{
    fn into_executor(self, _label: &L) -> Box<dyn SystemExecutor> {
        Box::new((self)())
    }
}

impl<L: ScheduleLabel + ?Sized, F: FnOnce() -> Box<dyn SystemExecutor>>
    IntoDefaultExecutor<L, ((), Box<dyn SystemExecutor>)> for F
{
    fn into_executor(self, _label: &L) -> Box<dyn SystemExecutor> {
        (self)()
    }
}

impl<L: ScheduleLabel + ?Sized, F: FnOnce(&L) -> E, E: SystemExecutor> IntoDefaultExecutor<L, &L>
    for F
{
    fn into_executor(self, label: &L) -> Box<dyn SystemExecutor> {
        Box::new((self)(label))
    }
}

impl<L: ScheduleLabel + ?Sized, F: FnOnce(&L) -> Box<dyn SystemExecutor>>
    IntoDefaultExecutor<L, (&L, Box<dyn SystemExecutor>)> for F
{
    fn into_executor(self, label: &L) -> Box<dyn SystemExecutor> {
        (self)(label)
    }
}

impl<L: ScheduleLabel, F: FnOnce(&dyn ScheduleLabel) -> E, E: SystemExecutor>
    IntoDefaultExecutor<L, &dyn ScheduleLabel> for F
{
    fn into_executor(self, label: &L) -> Box<dyn SystemExecutor> {
        Box::new((self)(label))
    }
}

impl<L: ScheduleLabel, F: FnOnce(&dyn ScheduleLabel) -> Box<dyn SystemExecutor>>
    IntoDefaultExecutor<L, (&dyn ScheduleLabel, Box<dyn SystemExecutor>)> for F
{
    fn into_executor(self, label: &L) -> Box<dyn SystemExecutor> {
        (self)(label)
    }
}
