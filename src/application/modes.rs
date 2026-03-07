use crate::AppState;

/// Set game mode - for manual transition from Consequences to Free
/// Scenario → Consequences is automatic only (via milestone with triggers_collapse)
/// Consequences → Free is manual (via this function)
/// Free → any is not allowed (one-way transition)
pub fn set_game_mode(
    state: &mut AppState,
    new_mode: crate::core::GameMode,
) -> Result<(), String> {
    let world_state = state.world_state.as_mut().ok_or("No active world state")?;
    let current_mode = world_state.game_mode;

    // Validate transitions
    match (current_mode, new_mode) {
        // Scenario → Consequences: automatic only, not allowed here
        (crate::core::GameMode::Scenario, _) => {
            return Err("Переход из Scenario возможен только автоматически при срабатывании milestone события".to_string());
        }
        // Consequences → Free: allowed
        (crate::core::GameMode::Consequences, crate::core::GameMode::Free) => {
            world_state.game_mode = crate::core::GameMode::Free;
            eprintln!("[GAME_MODE] Manual transition from Consequences to Free at tick {}", world_state.tick);
            Ok(())
        }
        // Any other transition: not allowed
        _ => {
            Err(format!("Недопустимый переход из {:?} в {:?}", current_mode, new_mode))
        }
    }
}
