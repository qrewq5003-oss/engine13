// TypeScript types matching Rust structures in engine13

// ============================================================================
// Core Types
// ============================================================================

export interface ActorMetrics {
  population: number;
  military_size: number;
  military_quality: number;
  economic_output: number;
  cohesion: number;
  legitimacy: number;
  external_pressure: number;
  treasury: number;
}

export interface Neighbor {
  id: string;
  distance: number;
  border_type: 'land' | 'sea';
}

export interface Successor {
  id: string;
  weight: number;
}

export interface GeoCoordinate {
  lat: number;
  lng: number;
}

export type NarrativeStatus = 'foreground' | 'background';
export type Era = 'ancient' | 'early_medieval' | 'high_medieval' | 'late_medieval' | 'early_modern';
export type RegionRank = 'D' | 'C' | 'B' | 'A' | 'S';

export interface Actor {
  id: string;
  name: string;
  name_short: string;
  region: string;
  region_rank: RegionRank;
  era: Era;
  narrative_status: NarrativeStatus;
  tags: string[];
  metrics: ActorMetrics;
  scenario_metrics: Record<string, number>;
  neighbors: Neighbor[];
  on_collapse: Successor[];
  actor_tags: Record<string, ActorTag>;
  center: GeoCoordinate | null;
}

export interface ActorTag {
  metrics_modifier: Record<string, number>;
  spreads_via: string[];
}

// ============================================================================
// World State
// ============================================================================

export type GameMode = 'scenario' | 'consequences' | 'free';

export interface DeadActor {
  id: string;
  tick_death: number;
  year_death: number;
  final_metrics: Record<string, number>;
  successor_ids: SuccessorWeight[];
}

export interface SuccessorWeight {
  id: string;
  weight: number;
}

export interface Alliance {
  actor_ids: string[];
  common_enemy: string | null;
  trade_benefit: boolean;
  formed_tick: number;
}

export interface WorldState {
  tick: number;
  year: number;
  scenario_id: string;
  game_mode: GameMode;
  actors: Record<string, Actor>;
  dead_actors: DeadActor[];
  alliances: Alliance[];
  milestone_events_fired: string[];
  family_metrics: Record<string, number>;
}

// ============================================================================
// Event Types
// ============================================================================

export type EventType =
  | 'collapse'
  | 'war'
  | 'migration'
  | 'threshold'
  | 'birth'
  | 'death'
  | 'trade'
  | 'cultural'
  | 'diplomatic'
  | 'player_action'
  | 'milestone';

export interface Event {
  id: string;
  tick: number;
  year: number;
  actor_id: string;
  type: EventType;
  is_key: boolean;
  description: string;
  involved_actors: string[];
  metrics_snapshot: Record<string, number>;
  tags: string[];
}

// ============================================================================
// Scenario Types
// ============================================================================

export interface Scenario {
  id: string;
  label: string;
  description: string;
  start_year: number;
  tempo: number;
  tick_span: number;
  era: Era;
  tick_label: string;
  actors: Actor[];
  auto_deltas: AutoDelta[];
  patron_actions: PatronAction[];
  milestone_events: MilestoneEvent[];
  rank_conditions: RankCondition[];
  generation_mechanics: GenerationMechanics | null;
  llm_context: string;
  consequence_context: string;
  player_actor_id: string | null;
}

export interface AutoDelta {
  metric: string;
  base: number;
  conditions: DeltaCondition[];
  noise: number;
}

export interface DeltaCondition {
  metric: string;
  operator: ComparisonOperator;
  value: number;
  delta: number;
}

export type ComparisonOperator = 'less' | 'less_or_equal' | 'greater' | 'greater_or_equal' | 'equal';

export interface PatronAction {
  id: string;
  name: string;
  available_if: ActionCondition;
  effects: Record<string, number>;
  cost: Record<string, number>;
}

export type ActionCondition = 
  | { type: 'always' }
  | { type: 'metric'; metric: string; operator: ComparisonOperator; value: number };

export interface MilestoneEvent {
  id: string;
  condition: EventCondition;
  is_key: boolean;
  triggers_collapse: boolean;
  llm_context_shift: string;
}

export interface EventCondition {
  type: 'metric' | 'actor_state' | 'tick';
  metric?: string;
  actor_id?: string;
  operator?: ComparisonOperator;
  value?: number;
  state?: 'dead' | 'alive' | 'foreground' | 'background';
  tick?: number;
  duration?: number;
}

export interface RankCondition {
  region_id: string;
  condition: EventCondition;
  result: RankResult;
  is_key: boolean;
}

export interface RankResult {
  rank: string;
}

export interface GenerationMechanics {
  tick_span: number;
  patriarch_start_age: number;
  patriarch_end_age: number;
}

export interface ScenarioMeta {
  id: string;
  label: string;
  description: string;
  start_year: number;
}

// ============================================================================
// Command Response Types
// ============================================================================

export interface LlmContext {
  current_year: number;
  current_tick: number;
  narrative_actors: string[];
  recent_events: string[];
  scenario_context: string;
}

export interface LlmTrigger {
  prompt: string;
  context: LlmContext;
}

export interface AdvanceTickResponse {
  world_state: WorldState;
  events: Event[];
  llm_trigger: LlmTrigger | null;
}

export interface SubmitActionResponse {
  success: boolean;
  effects: Record<string, number>;
  new_state: WorldState;
  llm_trigger: LlmTrigger | null;
  error: string | null;
}

export interface SaveResponse {
  success: boolean;
  save_id: string | null;
  error: string | null;
}

export interface LoadResponse {
  success: boolean;
  world_state: WorldState | null;
  error: string | null;
}

export interface SaveData {
  id: string;
  name: string;
  scenario_id: string;
  tick: number;
  year: number;
  created_at: number;
  world_state: WorldState;
  event_log: Event[];
}

// ============================================================================
// Family/Di Milano Specific Types
// ============================================================================

export interface FamilyMetrics {
  family_influence: number;
  family_knowledge: number;
  family_wealth: number;
  family_connections: number;
}

export interface Generation {
  number: number;
  patriarch_name: string;
  age: number;
  start_year: number;
}
