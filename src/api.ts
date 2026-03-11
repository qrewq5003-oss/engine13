// Tauri command invocations (Tauri v2)
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  WorldState,
  Actor,
  PatronAction,
  Event,
  SaveData,
  SaveSlotList,
  ScenarioMeta,
  AdvanceTickResponse,
  SubmitActionResponse,
  SaveResponse,
  StatusIndicatorState,
  HalfYear,
  ActionInfo,
  MapConfig,
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

// Streaming narrative - returns unsubscribe function
export async function getNarrative(
  onChunk: (text: string) => void,
  onDone: () => void,
  onError?: (error: string) => void,
  halfYear?: HalfYear
): Promise<() => void> {
  console.log('[API] Starting streaming narrative', halfYear ? `halfYear=${halfYear}` : '');

  // Listen for chunks
  const unlistenChunk = await listen<string>('narrative_chunk', (event) => {
    console.log('[API] narrative_chunk:', event.payload);
    onChunk(event.payload);
  });

  // Listen for done
  const unlistenDone = await listen('narrative_done', () => {
    console.log('[API] narrative_done');
    unlistenChunk();
    unlistenDone();
    onDone();
  });

  // Invoke the command with halfYear parameter
  try {
    await invoke('cmd_get_narrative', { halfYear });
  } catch (err) {
    console.error('[API] cmd_get_narrative error:', err);
    unlistenChunk();
    unlistenDone();
    if (onError) {
      onError(String(err));
    }
  }

  // Return unsubscribe function
  return () => {
    unlistenChunk();
    unlistenDone();
  };
}

export async function getAvailableModels(
  provider: string,
  base_url: string,
  api_key: string | null
): Promise<string[]> {
  return invoke<string[]>('cmd_get_available_models', { provider, baseUrl: base_url, apiKey: api_key });
}

export async function saveLlmConfig(
  provider: string,
  base_url: string,
  api_key: string | null,
  model: string
): Promise<void> {
  return invoke('cmd_save_llm_config', { provider, baseUrl: base_url, apiKey: api_key, model });
}

// ============================================================================
// Player Action Commands
// ============================================================================

export async function getAvailableActions(): Promise<PatronAction[]> {
  return invoke<PatronAction[]>('cmd_get_available_actions');
}

export async function getActionsWithAvailability(): Promise<ActionInfo[]> {
  return invoke<ActionInfo[]>('cmd_get_actions_with_availability');
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

export async function listSavesWithSlots(scenarioId: string): Promise<SaveSlotList> {
  return invoke<SaveSlotList>('cmd_list_saves_with_slots', { scenarioId });
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

export async function getStatusIndicators(): Promise<StatusIndicatorState[]> {
  return invoke<StatusIndicatorState[]>('cmd_get_status_indicators');
}

// ============================================================================
// Map Commands
// ============================================================================

export async function getMapConfig(): Promise<MapConfig | null> {
  return invoke<MapConfig | null>('cmd_get_map_config');
}
