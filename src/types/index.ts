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
  /// Global scenario metrics (e.g. federation_progress). Family metrics are in family_state.
  global_metrics: Record<string, number>;
  /// Feature flags for UI
  features?: ScenarioFeatures;
  /// Scenario start year for generation calculation
  scenario_start_year?: number;
  /// Generation length in years (for family scenarios)
  generation_length?: number;
  /// Actions taken this tick
  actions_this_tick: number;
  /// Actions allowed per tick (0 = unlimited)
  actions_per_tick: number;
  /// Victory achieved flag
  victory_achieved: boolean;
  /// Victory sustained ticks counter
  victory_sustained_ticks: number;
  /// Family state for family-based scenarios
  family_state?: FamilyState;
  /// Global metrics display configuration
  global_metrics_display?: MetricDisplay[];
  /// Generation mechanics (for family scenarios)
  generation_mechanics?: GenerationMechanics | null;
}

/// Family state for family-based scenarios
export interface FamilyState {
  metrics: Record<string, number>;
  patriarch_age: number;
}

/// Metric display configuration for UI
export interface MetricDisplay {
  metric: string;
  label: string;
  panel_title: string;
  thresholds: MetricThreshold[];
}

/// Threshold for metric display
export interface MetricThreshold {
  below: number;
  text: string;
}

/// Victory condition for scenario
export interface VictoryCondition {
  metric: string;
  threshold: number;
  title: string;
  description: string;
  minimum_tick: number;
  additional_conditions: Condition[];
  sustained_ticks_required: number;
}

/// Scenario feature flags for UI
export interface ScenarioFeatures {
  family_panel: boolean;
  global_metrics_panel: boolean;
  patron_actions: boolean;
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

/// Condition for random event triggering
export interface Condition {
  metric: string;
  operator: ComparisonOperator;
  value: number;
}

export type ComparisonOperator = 'less' | 'less_or_equal' | 'greater' | 'greater_or_equal' | 'equal';

/// Narrative season for dual-phase chronicle
export type NarrativeSeason = 'spring' | 'autumn';

export interface PatronAction {
  id: string;
  name: string;
  available_if: ActionCondition;
  effects: Record<string, number>;
  cost: Record<string, number>;
}

/// Reason why an action is unavailable
export type UnavailableReason =
  | { type: 'InsufficientCost'; required: number; available: number; resource: string }
  | { type: 'ActionsPerTickExhausted'; limit: number }
  | { type: 'ConditionNotMet'; description: string };

/// Action info with availability status
export interface ActionInfo {
  action: PatronAction;
  available: boolean;
  unavailable_reason?: UnavailableReason;
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

/// Era text for family panel context
export interface EraText {
  from_year: number;
  to_year: number;
  text: string;
}

export interface GenerationMechanics {
  tick_span: number;
  patriarch_start_age: number;
  patriarch_end_age: number;
  generation_length: number;
  panel_label: string;
  era_texts: EraText[];
}

export interface ScenarioMeta {
  id: string;
  label: string;
  description: string;
  start_year: number;
  victory_title?: string;
  victory_description?: string;
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

export interface SaveSlotData {
  id: string;
  name: string;
  scenario_id: string;
  tick: number;
  year: number;
  created_at: number;
  slot: string; // "auto" | "slot_1" | "slot_2" | "slot_3"
}

export interface SaveSlotList {
  auto: SaveSlotData | null;
  slots: Record<string, SaveSlotData>;
}

// ============================================================================
// Family/Di Milano Specific Types
// ============================================================================

// Note: FamilyMetrics replaced by FamilyState (metrics stored in family_state.metrics)

export interface Generation {
  number: number;
  patriarch_name: string;
  age: number;
  start_year: number;
}

// ============================================================================
// Status Panel Types
// ============================================================================

export interface StatusIndicatorState {
  label: string;
  value: number;
  status_text: string;
  progress: number; // 0.0-1.0
  invert: boolean;
}
