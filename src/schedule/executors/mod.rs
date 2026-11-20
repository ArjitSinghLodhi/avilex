use crate::{schedule::SystemsSchedule, world::World};

pub(crate) mod multi_threaded;
pub(crate) mod single_threaded;

/// Strategy trait defining how the systems inside a [`SystemsSchedule`] are initialized and executed.
///
/// [`SystemsSchedule`]: crate::app::schedule::SystemsSchedule
pub trait SystemExecutor: 'static {
    /// Prepares or partitions `schedule` before execution begins.
    fn init(&mut self, schedule: &mut SystemsSchedule);

    /// Executes the systems in `schedule` against `world`.
    fn run(&mut self, schedule: &mut SystemsSchedule, world: &mut World);
}

impl<T: SystemExecutor + 'static> From<T> for Box<dyn SystemExecutor> {
    fn from(executor: T) -> Self {
        Box::new(executor)
    }
}
