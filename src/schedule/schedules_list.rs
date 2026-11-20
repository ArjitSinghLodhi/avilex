use avilex_macros::ScheduleLabel;

use crate::{
    schedule::{SystemExecutor, SystemsSchedule},
    system::system_error::{SystemErrorHandler, default_system_error_handler},
    world::World,
};

/// The [`ScheduleLabel`] that runs once at application startup before the main frame loop begins.
///
/// Uses [`StartupExecutor`] as its default [`SystemExecutor`]; see its documentation for details
/// on how systems, commands, and despawns are executed during startup.
///
/// [`ScheduleLabel`]: crate::app::schedule::ScheduleLabel
/// [`SystemExecutor`]: crate::app::schedule::SystemExecutor
/// [`StartupExecutor`]: crate::app::schedule::StartupExecutor
#[derive(ScheduleLabel, Clone, PartialEq, Debug)]
#[schedule_label(default_executor = StartupExecutor)]
pub struct Startup;

/// The default [`SystemExecutor`] used by the [`Startup`] schedule.
///
/// Executes each eligible system sequentially in registration order. After every individual
/// system runs, queued commands ([`World::apply_queue_commands`]) and entity despawns
/// ([`World::apply_despawns`]) are flushed immediately so subsequent startup systems can observe
/// those changes.
///
/// System errors are forwarded to the [`SystemErrorHandler`] resource if present, falling back to
/// [`default_system_error_handler`].
///
/// [`SystemExecutor`]: crate::app::schedule::SystemExecutor
/// [`SystemErrorHandler`]: crate::app::system::system_error::SystemErrorHandler
/// [`default_system_error_handler`]: crate::app::system::system_error::default_system_error_handler
#[derive(Default)]
pub struct StartupExecutor;

impl SystemExecutor for StartupExecutor {
    fn init(&mut self, _schedule: &mut SystemsSchedule) {}
    fn run(&mut self, schedule: &mut SystemsSchedule, world: &mut World) {
        let handler = world
            .get_resource::<SystemErrorHandler>()
            .map(|handler| handler.0)
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

/// The [`ScheduleLabel`] for systems intended to run at the very start of each frame.
///
/// Ordered first in the frame loop by [`DefaultSchedulesPlugin`].
///
/// [`ScheduleLabel`]: crate::app::schedule::ScheduleLabel
/// [`DefaultSchedulesPlugin`]: crate::app::schedule::DefaultSchedulesPlugin
#[derive(ScheduleLabel, Clone, PartialEq, Debug)]
pub struct First;

/// The [`ScheduleLabel`] for systems intended to run before the main update logic.
///
/// Ordered between [`First`] and [`Update`] by [`DefaultSchedulesPlugin`].
///
/// [`ScheduleLabel`]: crate::app::schedule::ScheduleLabel
/// [`First`]: crate::app::schedule::First
/// [`Update`]: crate::app::schedule::Update
/// [`DefaultSchedulesPlugin`]: crate::app::schedule::DefaultSchedulesPlugin
#[derive(ScheduleLabel, Clone, PartialEq, Debug)]
pub struct PreUpdate;

/// The [`ScheduleLabel`] for core per-frame application logic.
///
/// Ordered between [`PreUpdate`] and [`PostUpdate`] by [`DefaultSchedulesPlugin`].
///
/// [`ScheduleLabel`]: crate::app::schedule::ScheduleLabel
/// [`PreUpdate`]: crate::app::schedule::PreUpdate
/// [`PostUpdate`]: crate::app::schedule::PostUpdate
/// [`DefaultSchedulesPlugin`]: crate::app::schedule::DefaultSchedulesPlugin
#[derive(ScheduleLabel, Clone, PartialEq, Debug)]
pub struct Update;

/// The [`ScheduleLabel`] for systems intended to run after the main update logic.
///
/// Ordered between [`Update`] and [`Last`] by [`DefaultSchedulesPlugin`].
///
/// [`ScheduleLabel`]: crate::app::schedule::ScheduleLabel
/// [`Update`]: crate::app::schedule::Update
/// [`Last`]: crate::app::schedule::Last
/// [`DefaultSchedulesPlugin`]: crate::app::schedule::DefaultSchedulesPlugin
#[derive(ScheduleLabel, Clone, PartialEq, Debug)]
pub struct PostUpdate;

/// The [`ScheduleLabel`] for systems intended to run at the very end of each frame.
///
/// Ordered last in the frame loop by [`DefaultSchedulesPlugin`].
///
/// [`ScheduleLabel`]: crate::app::schedule::ScheduleLabel
/// [`DefaultSchedulesPlugin`]: crate::app::schedule::DefaultSchedulesPlugin
#[derive(ScheduleLabel, Clone, PartialEq, Debug)]
pub struct Last;
