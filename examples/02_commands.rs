use avilex::prelude::*;
use std::thread;
use std::time::Duration;

fn test_runner(app: &mut App) {
    app.build();
    app.run_startup();

    println!("--- Frame 1 ---");
    app.update();
    println!("--- Frame 2 ---");
    app.update();
}

#[derive(Component)]
struct Position {
    x: f32,
    y: f32,
}

#[derive(Component)]
struct Speed(f32);

#[derive(Component)]
struct ThreadMarker(u32);

fn main() {
    let mut app = App::new();
    app.add_systems(Startup, queue_spawn_commands_system)
        .add_systems(Startup, system_parallel_spawn_system)
        .add_systems(Startup, setup_mid_apply_test)
        .add_systems(Update, verify_spawned_entities_system)
        .set_runner(test_runner);

    let commands = app.world().commands();
    let thread_1_cmds = commands.clone();
    let thread_2_cmds = commands.clone();

    thread::scope(|s| {
        s.spawn(move || {
            thread::sleep(Duration::from_millis(5));
            println!("[Thread 1] Queueing 500 entities...");
            for i in 0..500 {
                thread_1_cmds.spawn((
                    Position {
                        x: i as f32,
                        y: 1.0,
                    },
                    Speed(2.0),
                    ThreadMarker(1),
                ));
            }
        });

        s.spawn(move || {
            thread::sleep(Duration::from_millis(5));
            println!("[Thread 2] Queueing 500 entities...");
            for i in 0..500 {
                thread_2_cmds.spawn((
                    Position {
                        x: i as f32,
                        y: 2.0,
                    },
                    Speed(2.0),
                    ThreadMarker(2),
                ));
            }
        });
    });

    app.run();
}

fn queue_spawn_commands_system(commands: Commands) {
    println!("Queueing a single entity spawn via a component tuple...");
    commands.spawn((Position { x: 0.0, y: 0.0 }, Speed(5.0)));

    println!("Queueing a batch spawn of 1,000 entities via tuple iterator...");
    let batch = (0..1000).map(|i| {
        (
            Position {
                x: i as f32,
                y: 10.0,
            },
            Speed(1.0),
        )
    });
    commands.spawn_batch(batch);
}

fn system_parallel_spawn_system(commands: Commands) {
    let thread_3_cmds = commands.clone();
    thread::scope(|s| {
        s.spawn(move || {
            println!("[System-Injected Thread 3] Queueing 500 entities...");
            for i in 0..500 {
                thread_3_cmds.spawn((
                    Position {
                        x: i as f32,
                        y: 3.0,
                    },
                    Speed(3.0),
                    ThreadMarker(3),
                ));
            }
        });
    });
}

fn setup_mid_apply_test(commands: Commands) {
    let mid_apply_cmds = commands.clone();

    commands.queue(move |_world: &mut World| {
        let nested_cmds = mid_apply_cmds.clone();
        println!("[World Apply Loop] Queueing 500 entities mid-apply...");

        thread::scope(|s| {
            s.spawn(move || {
                for i in 0..500 {
                    nested_cmds.spawn((
                        Position {
                            x: i as f32,
                            y: 4.0,
                        },
                        Speed(4.0),
                        ThreadMarker(4),
                    ));
                }
            });
        });
    });
}

fn verify_spawned_entities_system(
    query: Query<(&Position, &Speed)>,
    marker_query: Query<&ThreadMarker>,
) {
    let mut total_count = 0;
    for (pos, speed) in query.iter() {
        total_count += 1;
        if total_count == 1 || total_count == 1001 {
            println!(
                "Inspecting Entity: Position({}, {}), Speed: {}",
                pos.x, pos.y, speed.0
            );
        }
    }

    let mut t1_count = 0;
    let mut t2_count = 0;
    let mut t3_count = 0;
    let mut t4_count = 0;

    for marker in marker_query.iter() {
        if marker.0 == 1 {
            t1_count += 1;
        } else if marker.0 == 2 {
            t2_count += 1;
        } else if marker.0 == 3 {
            t3_count += 1;
        } else if marker.0 == 4 {
            t4_count += 1;
        }
    }

    if t1_count > 0 || t2_count > 0 || t3_count > 0 || t4_count > 0 {
        println!("\n>> avilex Execution Sync Analysis:");
        println!(
            "   -> Cloned Handles Total Verified Entities: {}",
            total_count
        );
        println!("   -> Thread 1: {}", t1_count);
        println!("   -> Thread 2: {}", t2_count);
        println!("   -> System Thread 3: {}", t3_count);
        println!("   -> Mid-Apply Thread 4: {}", t4_count);

        assert_eq!(t1_count, 500);
        assert_eq!(t2_count, 500);
        assert_eq!(t3_count, 500);
        assert_eq!(t4_count, 500);
        println!("   -> Success: All uniform commands applied perfectly!");
    }
}
