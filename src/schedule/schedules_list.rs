use avilex_macros::ScheduleLabel;

use crate::{
    schedule::{SystemExecutor, SystemsSchedule},
    system::system_error::{SystemErrorHandler, default_system_error_handler},
    world::World,
};

/// A schedule where queue commands and despawns are applied between each system that runs registered in this schedule.
#[derive(ScheduleLabel, Clone, PartialEq, Debug)]
#[schedule_label(default_executor = StartupExecutor::new)]
pub struct Startup;

pub(crate) struct StartupExecutor;

impl StartupExecutor {
    fn new(_label: &mut Startup) -> Self {
        Self
    }
}

impl SystemExecutor for StartupExecutor {
    fn init(&mut self, _schedule: &mut SystemsSchedule) {}
    fn run(&mut self, schedule: &mut SystemsSchedule, world: &mut World) {
        let handler = world
            .get_resource::<SystemErrorHandler>()
            .map(|handler| handler.handler())
            .unwrap_or(default_system_error_handler);
        for system in schedule.systems_mut() {
            if system.should_run(world) {
                let result = system.run((), world);
                if let Err(err) = result {
                    handler(err)
                }
            }
            world.apply_queue_commands();
            world.apply_despawns();
        }
    }
}

#[derive(ScheduleLabel, Clone, PartialEq, Debug)]
pub struct First;

#[derive(ScheduleLabel, Clone, PartialEq, Debug)]
pub struct PreUpdate;

#[derive(ScheduleLabel, Clone, PartialEq, Debug)]
pub struct Update;

#[derive(ScheduleLabel, Clone, PartialEq, Debug)]
pub struct PostUpdate;

#[derive(ScheduleLabel, Clone, PartialEq, Debug)]
pub struct Last;
