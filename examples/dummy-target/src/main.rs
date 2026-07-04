use std::{thread, time::Duration};

#[repr(C)]
struct GameState {
    health: i32,
    coins: i32,
    stamina: f32,
}

fn main() {
    let mut state = Box::new(GameState {
        health: 100,
        coins: 50,
        stamina: 75.0,
    });

    println!("dummy-target pid={}", std::process::id());
    println!("health address={:p} value={}", &state.health, state.health);
    println!("coins address={:p} value={}", &state.coins, state.coins);
    println!("stamina address={:p} value={}", &state.stamina, state.stamina);
    println!("Use these stable changing values to test scan/write/freeze.");

    loop {
        state.health = if state.health <= 10 { 100 } else { state.health - 1 };
        state.coins += 1;
        state.stamina = if state.stamina <= 1.0 { 75.0 } else { state.stamina - 0.5 };
        println!(
            "health={} coins={} stamina={:.1}",
            state.health, state.coins, state.stamina
        );
        thread::sleep(Duration::from_secs(2));
    }
}
