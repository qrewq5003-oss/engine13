// Tauri command invocations (Tauri v2)
import { invoke } from '@tauri-apps/api/core';
import type {
  WorldState,
  Actor,
  PatronAction,
  Event,
  SaveData,
  ScenarioMeta,
  AdvanceTickResponse,
  SubmitActionResponse,
  SaveResponse,
} from './types/index';

// ============================================================================
// Simulation Commands
// ============================================================================

export async function getWorldState(): Promise<WorldState | null> {
  console.log('[API] Calling cmd_get_world_state');
  const result = await invoke<WorldState | null>('cmd_get_world_state');
  console.log('[API] Got world state:', result ? 'loaded' : 'null');
  return result;
}

export async function advanceTick(actionId?: string): Promise<AdvanceTickResponse> {
  const action = actionId ? { actionId, targetActorId: null } : null;
  return invoke<AdvanceTickResponse>('cmd_advance_tick', { action });
}

export async function getNarrativeActors(): Promise<Actor[]> {
  return invoke<Actor[]>('cmd_get_narrative_actors');
}

// ============================================================================
// Player Action Commands
// ============================================================================

export async function getAvailableActions(): Promise<PatronAction[]> {
  return invoke<PatronAction[]>('cmd_get_available_actions');
}

export async function submitAction(actionId: string): Promise<SubmitActionResponse> {
  console.log('[API] Calling cmd_submit_action with actionId:', actionId);
  const result = invoke<SubmitActionResponse>('cmd_submit_action', { actionId });
  console.log('[API] cmd_submit_action called, waiting for response...');
  return result;
}

// ============================================================================
// Save/Load Commands
// ============================================================================

export async function saveGame(slot?: string, name?: string): Promise<SaveResponse> {
  return invoke<SaveResponse>('cmd_save_game', { slot, name });
}

export async function loadGame(saveId: string): Promise<SaveResponse> {
  return invoke<SaveResponse>('cmd_load_game', { saveId });
}

export async function listSaves(): Promise<SaveData[]> {
  return invoke<SaveData[]>('cmd_list_saves');
}

// ============================================================================
// History Commands
// ============================================================================

export async function getRelevantEvents(actorIds: string[]): Promise<Event[]> {
  return invoke<Event[]>('cmd_get_relevant_events', { actorIds });
}

// ============================================================================
// Scenario Commands
// ============================================================================

export async function loadScenario(scenarioId: string): Promise<SaveResponse> {
  console.log('[API] Calling cmd_load_scenario with:', scenarioId);
  const result = await invoke<SaveResponse>('cmd_load_scenario', { scenarioId });
  console.log('[API] load_scenario result:', result);
  return result;
}

export async function getScenarioList(): Promise<ScenarioMeta[]> {
  return invoke<ScenarioMeta[]>('cmd_get_scenario_list');
}
