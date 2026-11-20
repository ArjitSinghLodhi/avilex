use avilex::{
    app::schedule::{MultiThreadedExecutor, Schedule, SingleThreadedExecutor, SystemExecutor},
    prelude::*,
};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[derive(Debug, PartialEq, Eq)]
enum ExecutedSystem {
    First,
    Second,
    Third,
}

static CAPTURED_THREADS: AtomicUsize = AtomicUsize::new(0);
static GATE_ENABLED: AtomicBool = AtomicBool::new(true);
static GATE_EVALUATIONS: AtomicUsize = AtomicUsize::new(0);
static EXECUTION_ORDER: Mutex<Vec<ExecutedSystem>> = Mutex::new(Vec::new());

fn create_custom_executor(label: &mut GameplaySchedule) -> MultiThreadedExecutor {
    if let GameplaySchedule::Update { threads } = label {
        CAPTURED_THREADS.store(*threads, Ordering::Relaxed);
    }
    MultiThreadedExecutor::new()
}

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
#[schedule_label(default_executor = create_custom_executor)]
enum GameplaySchedule {
    Update { threads: usize },
    PostUpdate,
}

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
#[schedule_label(default_executor)]
struct ShorthandBoxedSchedule {
    use_single_thread: bool,
}

impl ShorthandBoxedSchedule {
    fn default_executor(&mut self) -> Box<dyn SystemExecutor> {
        if self.use_single_thread {
            Box::new(SingleThreadedExecutor::new())
        } else {
            Box::new(MultiThreadedExecutor::new())
        }
    }
}

#[derive(SystemSet, Clone, Debug, PartialEq, Eq)]
struct SharedSet;

fn check_gate() -> bool {
    GATE_EVALUATIONS.fetch_add(1, Ordering::Relaxed);
    GATE_ENABLED.load(Ordering::Relaxed)
}

fn first_system() {
    EXECUTION_ORDER.lock().push(ExecutedSystem::First);
}

fn second_system() {
    EXECUTION_ORDER.lock().push(ExecutedSystem::Second);
}

fn third_system() {
    EXECUTION_ORDER.lock().push(ExecutedSystem::Third);
}

#[test_fork::test]
fn test_data_based_schedules_and_execution_order() {
    EXECUTION_ORDER.lock().clear();

    App::new()
        .configure_schedule_order(
            GameplaySchedule::Update { threads: 4 },
            GameplaySchedule::PostUpdate,
        )
        .configure_sets(GameplaySchedule::Update { threads: 4 }, SharedSet)
        .configure_sets(GameplaySchedule::PostUpdate, SharedSet)
        .add_systems(
            GameplaySchedule::Update { threads: 4 },
            first_system.in_set(SharedSet),
        )
        .add_systems(
            GameplaySchedule::PostUpdate,
            second_system.in_set(SharedSet),
        )
        .run();

    assert_eq!(
        *EXECUTION_ORDER.lock(),
        vec![ExecutedSystem::First, ExecutedSystem::Second]
    );
}

#[test_fork::test]
fn test_schedule_label_custom_and_shorthand_executors() {
    CAPTURED_THREADS.store(0, Ordering::Relaxed);

    let mut concrete_label = GameplaySchedule::Update { threads: 8 };
    let _executor = concrete_label.default_executor();
    assert_eq!(CAPTURED_THREADS.load(Ordering::Relaxed), 8);

    let mut boxed_label = ShorthandBoxedSchedule {
        use_single_thread: true,
    };
    let _boxed_executor = boxed_label.default_executor();
}

#[test_fork::test]
fn test_app_schedule_crud_and_overwrite() {
    EXECUTION_ORDER.lock().clear();

    let label = GameplaySchedule::Update { threads: 2 };
    let mut app = App::new();

    assert!(app.get_schedule(label.clone()).is_none());

    app.init_schedule(label.clone())
        .add_systems(label.clone(), first_system)
        .init_schedule(label.clone());
    assert!(app.get_schedule(label.clone()).is_some());

    app.edit_schedule(label.clone(), |schedule| {
        schedule.add_systems(second_system);
    });

    app.get_schedule_mut(label.clone())
        .unwrap()
        .add_systems(third_system);

    let replacement = Schedule::new(label.clone()).with_systems(third_system);
    app.insert_schedule(replacement);

    app.init_schedule(GameplaySchedule::PostUpdate)
        .configure_schedule_order(label, GameplaySchedule::PostUpdate)
        .run();

    assert_eq!(*EXECUTION_ORDER.lock(), vec![ExecutedSystem::Third]);
}

#[test_fork::test]
fn test_standalone_schedule_initialize_dirty_and_rebuild_sets() {
    EXECUTION_ORDER.lock().clear();
    GATE_ENABLED.store(true, Ordering::Relaxed);
    GATE_EVALUATIONS.store(0, Ordering::Relaxed);

    let mut app = App::empty();
    let world = app.world_mut();
    let mut schedule = Schedule::new(GameplaySchedule::Update { threads: 1 })
        .with_configured_sets(SharedSet)
        .with_systems(second_system.in_set(SharedSet));

    schedule.initialize();
    schedule.run(world);
    assert_eq!(*EXECUTION_ORDER.lock(), vec![ExecutedSystem::Second]);

    EXECUTION_ORDER.lock().clear();
    schedule
        .configure_sets(SharedSet.run_if(check_gate))
        .add_systems((
            first_system.before(SharedSet),
            third_system.in_set(SharedSet).after(second_system),
        ));
    schedule.run(world);
    assert_eq!(
        *EXECUTION_ORDER.lock(),
        vec![
            ExecutedSystem::First,
            ExecutedSystem::Second,
            ExecutedSystem::Third
        ]
    );
    assert_eq!(GATE_EVALUATIONS.load(Ordering::Relaxed), 1);

    EXECUTION_ORDER.lock().clear();
    schedule.systems_schedule_mut().systems_mut().reverse();
    schedule.set_dirty();
    schedule.run(world);
    assert_eq!(
        *EXECUTION_ORDER.lock(),
        vec![
            ExecutedSystem::First,
            ExecutedSystem::Second,
            ExecutedSystem::Third
        ]
    );
    assert_eq!(GATE_EVALUATIONS.load(Ordering::Relaxed), 2);

    EXECUTION_ORDER.lock().clear();
    GATE_ENABLED.store(false, Ordering::Relaxed);
    schedule.run(world);
    assert_eq!(*EXECUTION_ORDER.lock(), vec![ExecutedSystem::First]);
}
