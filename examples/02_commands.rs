use avilex::prelude::*;

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

fn main() {
    App::new()
        .add_systems(Startup, queue_spawn_commands_system)
        .add_systems(Update, verify_spawned_entities_system)
        .set_runner(test_runner)
        .run();
}

fn queue_spawn_commands_system(commands: Commands, par_commands: ParallelCommands) {
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

    println!("Spawning entities in parallel via ParallelCommands...");
    std::thread::scope(|s| {
        s.spawn(|| {
            par_commands.scope(|cmd| {
                cmd.spawn((Position { x: -10.0, y: -10.0 }, Speed(2.5)));
            });
        });
        s.spawn(|| {
            par_commands.scope(|cmd| {
                cmd.spawn((Position { x: -20.0, y: -20.0 }, Speed(3.5)));
            });
        });
    });

    println!("Queueing a deferred closure execution via commands.queue...");
    commands.queue(move |_: &mut World| {
        println!("Executing deferred commands.queue closure...");
        par_commands.scope(|cmd| {
            cmd.spawn((Position { x: 999.0, y: 999.0 }, Speed(99.0)));
        });
    });
}

fn verify_spawned_entities_system(query: Query<(&Position, &Speed)>) {
    let mut count = 0;
    let mut found_standard = false;
    let mut found_batch = 0;
    let mut found_parallel = 0;
    let mut found_queued = false;

    for (pos, speed) in query.iter() {
        count += 1;

        if pos.x == 0.0 && pos.y == 0.0 && speed.0 == 5.0 {
            found_standard = true;
        }
        if pos.y == 10.0 && speed.0 == 1.0 {
            found_batch += 1;
        }
        if (pos.x == -10.0 && pos.y == -10.0 && speed.0 == 2.5)
            || (pos.x == -20.0 && pos.y == -20.0 && speed.0 == 3.5)
        {
            found_parallel += 1;
        }
        if pos.x == 999.0 && pos.y == 999.0 && speed.0 == 99.0 {
            found_queued = true;
        }
    }

    println!("Verified total entities in query: {}", count);

    assert!(found_standard);
    assert_eq!(found_batch, 1000);
    assert_eq!(found_parallel, 2);
    assert!(found_queued);
}
