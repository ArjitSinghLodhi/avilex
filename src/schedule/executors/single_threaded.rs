use crate::{
    schedule::{SystemExecutor, SystemsSchedule},
    system::system_error::{SystemErrorHandler, default_system_error_handler},
    world::World,
};

#[derive(Default)]
pub struct SingleThreadedExecutor;

impl SystemExecutor for SingleThreadedExecutor {
    fn init(&mut self, _schedule: &mut SystemsSchedule) {}
    fn run(&mut self, schedule: &mut SystemsSchedule, world: &mut World) {
        let handler = world
            .get_resource::<SystemErrorHandler>()
            .map(|handler| handler.handler())
            .unwrap_or(default_system_error_handler);
        for node in schedule.systems_mut() {
            if node.should_run(world) {
                let result = node.run((), world);
                if let Err(err) = result {
                    handler(err);
                }
            }
        }
    }
}
