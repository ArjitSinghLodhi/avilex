use crate::{
    schedule::{SystemsSchedule, executors::SystemExecutor},
    system::system_error::{SystemErrorHandler, default_system_error_handler},
    world::World,
};

/// A [`SystemExecutor`] that runs all systems sequentially on the main thread in their scheduled
/// order.
///
/// Any errors returned by systems are forwarded immediately to the handler stored in the
/// [`SystemErrorHandler`] resource, falling back to [`default_system_error_handler`] if none is
/// inserted.
///
/// [`SystemExecutor`]: crate::app::schedule::SystemExecutor
/// [`SystemErrorHandler`]: crate::app::system::system_error::SystemErrorHandler
/// [`default_system_error_handler`]: crate::app::system::system_error::default_system_error_handler
#[derive(Default)]
pub struct SingleThreadedExecutor;

impl SingleThreadedExecutor {
    /// Creates a new [`SingleThreadedExecutor`].
    pub const fn new() -> Self {
        Self
    }
}

impl SystemExecutor for SingleThreadedExecutor {
    fn init(&mut self, _schedule: &mut SystemsSchedule) {}
    fn run(&mut self, schedule: &mut SystemsSchedule, world: &mut World) {
        let handler = world
            .get_resource::<SystemErrorHandler>()
            .map(|handler| handler.0)
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
