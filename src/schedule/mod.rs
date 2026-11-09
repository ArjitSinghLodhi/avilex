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

pub trait ScheduleLabel: Any + Send + Sync + DynEq + Debug {
    fn default_executor(&mut self) -> Box<dyn SystemExecutor> {
        Box::new(MultiThreadedExecutor::new())
    }

    fn dyn_clone(&self) -> Box<dyn ScheduleLabel>;
}

pub struct SystemsSchedule {
    systems: Vec<SystemNode<(), ()>>,
}

impl SystemsSchedule {
    pub(crate) fn new() -> Self {
        Self {
            systems: Vec::new(),
        }
    }

    pub fn systems(&self) -> &Vec<SystemNode<(), ()>> {
        &self.systems
    }

    pub fn systems_mut(&mut self) -> &mut Vec<SystemNode<(), ()>> {
        &mut self.systems
    }
}

pub trait SystemExecutor: Send + Sync + 'static {
    fn init(&mut self, schedule: &mut SystemsSchedule);
    fn run(&mut self, schedule: &mut SystemsSchedule, world: &mut World);
}

pub struct Schedule {
    schedule: Box<dyn ScheduleLabel>,
    systems_schedule: SystemsSchedule,
    executor: Box<dyn SystemExecutor>,
}

impl Schedule {
    pub fn new<L: ScheduleLabel + 'static>(mut label: L) -> Self {
        Self {
            executor: label.default_executor(),
            schedule: Box::new(label),
            systems_schedule: SystemsSchedule::new(),
        }
    }

    pub fn with_executor(self, executor: impl SystemExecutor + 'static) -> Self {
        self.with_executor_boxed(Box::new(executor))
    }

    pub fn with_executor_boxed(mut self, executor: Box<dyn SystemExecutor + 'static>) -> Self {
        self.set_executor_boxed(executor);
        self
    }

    pub fn set_executor(&mut self, executor: impl SystemExecutor + 'static) {
        self.set_executor_boxed(Box::new(executor));
    }

    pub fn set_executor_boxed(&mut self, executor: Box<dyn SystemExecutor + 'static>) {
        self.executor = executor;
    }

    pub fn add_system(&mut self, system: impl System<In = (), Out = ()> + 'static) {
        self.systems_schedule.systems.push(SystemNode::new(system));
    }

    pub fn add_system_boxed(&mut self, system: Box<dyn System<In = (), Out = ()>>) {
        self.systems_schedule
            .systems
            .push(SystemNode::new_boxed(system));
    }

    pub fn systems_schedule(&self) -> &SystemsSchedule {
        &self.systems_schedule
    }

    pub fn systems_schedule_mut(&mut self) -> &mut SystemsSchedule {
        &mut self.systems_schedule
    }

    pub fn label(&self) -> &dyn ScheduleLabel {
        &*self.schedule
    }

    pub fn label_mut(&mut self) -> &mut dyn ScheduleLabel {
        &mut *self.schedule
    }

    pub(crate) fn init_executor(&mut self) {
        self.executor.init(&mut self.systems_schedule);
    }

    pub fn run(&mut self, world: &mut World) {
        self.executor.run(&mut self.systems_schedule, world);
    }
}

pub(crate) struct ScheduleConstraint {
    pub(crate) before: Box<dyn ScheduleLabel>,
    pub(crate) after: Box<dyn ScheduleLabel>,
}
