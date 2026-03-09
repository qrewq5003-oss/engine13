pub mod application;
pub mod core;
pub mod engine;
pub mod events;
pub mod llm;
pub mod scenarios;
pub mod commands;
pub mod db;

#[cfg(test)]
mod tests;

pub use core::*;
pub use engine::{EventLog, TickExplanation, generate_tick_explanation};
pub use commands::{AppState, SaveData, AdvanceTickResponse, SubmitActionResponse, SaveResponse, LoadResponse, ScenarioMeta};
pub use db::{Db, DbSave, DbDeadActor};
pub use llm::{LlmConfig, LlmTrigger, LlmContext, TriggerType, HalfYear, get_llm_config, save_llm_config, generate_narrative_prompt, stream_narrative_anthropic, stream_narrative_openai, get_available_models};
pub use application::{list_saves, list_saves_with_slots, load_game, load_scenario, save_game, set_game_mode, get_available_actions, submit_action, cmd_get_narrative, SaveSlotData, SaveSlotList};
pub use scenarios::{get_scenario_list, get_scenario_meta};
