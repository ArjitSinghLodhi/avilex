use crate::{
    schedule::{ScheduleLabel, SystemExecutor, SystemsSchedule},
    system::system_error::{SystemErrorHandler, default_system_error_handler},
    world::World,
};

/// A schedule where queue commands and despawns are applied between each system that runs registered in this schedule.
#[derive(PartialEq, Eq, Debug, Clone)]
pub struct Startup;

impl ScheduleLabel for Startup {
    fn default_executor(&mut self) -> Box<dyn SystemExecutor> {
        Box::new(StartupExecutor)
    }

    fn dyn_clone(&self) -> Box<dyn ScheduleLabel> {
        Box::new(self.clone())
    }
}

pub(crate) struct StartupExecutor;

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

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct First;

impl ScheduleLabel for First {
    fn dyn_clone(&self) -> Box<dyn ScheduleLabel> {
        Box::new(self.clone())
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct PreUpdate;

impl ScheduleLabel for PreUpdate {
    fn dyn_clone(&self) -> Box<dyn ScheduleLabel> {
        Box::new(self.clone())
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct Update;

impl ScheduleLabel for Update {
    fn dyn_clone(&self) -> Box<dyn ScheduleLabel> {
        Box::new(self.clone())
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct PostUpdate;

impl ScheduleLabel for PostUpdate {
    fn dyn_clone(&self) -> Box<dyn ScheduleLabel> {
        Box::new(self.clone())
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct Last;

impl ScheduleLabel for Last {
    fn dyn_clone(&self) -> Box<dyn ScheduleLabel> {
        Box::new(self.clone())
    }
}
