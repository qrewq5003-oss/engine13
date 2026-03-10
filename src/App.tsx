import React, { useState, useEffect, useCallback } from 'react';
import { WorldPanel } from './components/WorldPanel';
import { FamilyPanel } from './components/FamilyPanel';
import { GlobalMetricsPanel } from './components/GlobalMetricsPanel';
import { StatusPanel } from './components/StatusPanel';
import { ActionHistory } from './components/ActionHistory';
import { ControlPanel } from './components/ControlPanel';
import { NarrativePanel } from './components/NarrativePanel';
import { SettingsPanel } from './components/SettingsPanel';
import { ScenarioSelectScreen } from './components/ScenarioSelectScreen';
import { SaveSlotModal } from './components/SaveSlotModal';
import VictoryScreen from './components/VictoryScreen';
import {
  loadScenario,
  getWorldState,
  getActionsWithAvailability,
  advanceTick,
  submitAction,
  getRelevantEvents,
  getNarrative,
  getScenarioList,
  listSaves,
  listSavesWithSlots,
  loadGame,
  saveGame,
  getStatusIndicators,
} from './api';
import type { WorldState, Actor, Event, ScenarioMeta, SaveSlotData, SaveSlotList, StatusIndicatorState, HalfYear, ActionInfo } from './types';
import './App.css';

const App: React.FC = () => {
  // Game state: 'menu' or 'playing'
  const [gameState, setGameState] = useState<'menu' | 'playing'>('menu');
  
  // Menu state
  const [scenarios, setScenarios] = useState<ScenarioMeta[]>([]);
  const [hasSaves, setHasSaves] = useState(false);
  
  // Game state
  const [worldState, setWorldState] = useState<WorldState | null>(null);
  const [availableActions, setAvailableActions] = useState<ActionInfo[]>([]);
  const [recentEvents, setRecentEvents] = useState<Event[]>([]);
  const [selectedActorId, setSelectedActorId] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [scenarioLoaded, setScenarioLoaded] = useState(false);
  const [loadingStep, setLoadingStep] = useState<string>('Initializing...');

  // Narrative state
  const [narrative, setNarrative] = useState<string | null>(null);
  const [narrativeLoading, setNarrativeLoading] = useState(false);
  const [isGeneratingNarrative, setIsGeneratingNarrative] = useState(false);

  // Status panel state
  const [statusIndicators, setStatusIndicators] = useState<StatusIndicatorState[]>([]);

  // Settings state
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);

  // Initialize menu on mount
  useEffect(() => {
    const initMenu = async () => {
      try {
        console.log('[App] Starting menu initialization');
        const [scenarioList, saves] = await Promise.all([
          getScenarioList(),
          listSaves(),
        ]);
        setScenarios(scenarioList);
        setHasSaves(saves.length > 0);
        console.log('[App] Menu initialized:', scenarioList.length, 'scenarios,', saves.length, 'saves');
      } catch (err) {
        console.error('[App] Menu initialization error:', err);
        setError(`Failed to initialize menu: ${err}`);
      }
    };

    initMenu();
  }, []);

  // Handle starting a scenario
  const handleStartScenario = async (scenarioId: string) => {
    try {
      setIsLoading(true);
      setLoadingStep('Loading scenario...');

      // Reset UI state for clean start
      setNarrative("");
      setRecentEvents([]);

      const loadResult = await loadScenario(scenarioId);
      if (!loadResult.success) {
        throw new Error(loadResult.error || 'Failed to load scenario');
      }

      setLoadingStep('Fetching world state...');
      await refreshState();
      await refreshNarrative();

      setLoadingStep('Complete');
      setScenarioLoaded(true);
      setGameState('playing');
    } catch (err) {
      console.error('[App] Start scenario error:', err);
      setError(`Failed to load scenario: ${err}`);
    } finally {
      setIsLoading(false);
    }
  };

  // Handle continuing from save
  const handleContinue = async (saves: SaveSlotData[]) => {
    try {
      setIsLoading(true);
      setLoadingStep('Loading save...');

      // Sort by tick descending to get latest save
      const sortedSaves = [...saves].sort((a, b) => b.tick - a.tick);
      const latestSave = sortedSaves[0];

      if (!latestSave) {
        throw new Error('No saves found');
      }

      const loadResult = await loadGame(latestSave.id);
      if (!loadResult.success) {
        throw new Error(loadResult.error || 'Failed to load save');
      }

      setLoadingStep('Fetching world state...');
      await refreshState();
      await refreshNarrative();

      setLoadingStep('Complete');
      setScenarioLoaded(true);
      setGameState('playing');
    } catch (err) {
      console.error('[App] Continue error:', err);
      setError(`Failed to load save: ${err}`);
    } finally {
      setIsLoading(false);
    }
  };

  // Refresh all state
  const refreshState = useCallback(async () => {
    try {
      const [world, actions] = await Promise.all([
        getWorldState(),
        getActionsWithAvailability(),
      ]);

      if (world) {
        setWorldState(world);
        // Get recent events for narrative actors
        const actors = Object.values(world.actors) as Actor[];
        const narrativeActorIds = actors
          .filter(a => a.narrative_status === 'foreground')
          .map(a => a.id);
        const events = await getRelevantEvents(narrativeActorIds);
        setRecentEvents(events);

        // Get status indicators
        try {
          const indicators = await getStatusIndicators();
          setStatusIndicators(indicators);
        } catch (err) {
          console.warn('Failed to get status indicators:', err);
        }
      }

      setAvailableActions(actions || []);
    } catch (err) {
      setError(`Failed to refresh state: ${err}`);
    }
  }, []);

  // Refresh narrative (streaming - uses current world state from API)
  const refreshNarrative = useCallback(async (stateForNarrative?: WorldState) => {
    try {
      setNarrativeLoading(true);
      setIsGeneratingNarrative(true);
      setNarrative(''); // Reset narrative before streaming

      // Use provided state or fall back to current worldState
      const currentState = stateForNarrative ?? worldState;

      // Determine half-year based on tick - even ticks are FirstHalf, odd are SecondHalf
      const halfYear: HalfYear = currentState && currentState.tick % 2 === 0 ? 'first_half' : 'second_half';

      await getNarrative(
        (chunk) => {
          // Append each chunk to narrative
          setNarrative((prev) => (prev || '') + chunk);
        },
        () => {
          // Done streaming
          console.log('[Narrative] Streaming complete');
          setNarrativeLoading(false);
          setIsGeneratingNarrative(false);
        },
        (error) => {
          // Error handling - use placeholder
          console.error('[Narrative] Error:', error);
          const placeholder = currentState
            ? `${currentState.year} год. Хроника продолжается.`
            : 'Хроника продолжается.';
          setNarrative(placeholder);
          setNarrativeLoading(false);
          setIsGeneratingNarrative(false);
        },
        halfYear
      );
    } catch (err) {
      console.error('[Narrative] Error:', err);
      // Don't show error - LLM may be unavailable, use placeholder
      const placeholder = worldState
        ? `${worldState.year} год. Хроника продолжается.`
        : 'Хроника продолжается.';
      setNarrative(placeholder);
      setNarrativeLoading(false);
      setIsGeneratingNarrative(false);
    }
  }, [worldState]);

  // Handle advance tick - correct order: advanceTick first, then getNarrative every tick
  const handleAdvanceTick = useCallback(async () => {
    if (isLoading || isGeneratingNarrative) return;

    try {
      setIsLoading(true);
      setError(null);

      // Step 1: Advance tick - updates world_state
      const response = await advanceTick();
      setWorldState(response.world_state);
      if (response.events.length > 0) {
        setRecentEvents(response.events);
      }

      // Step 2: Refresh available actions and events to reflect new state
      await refreshState();

      // Step 3: Generate narrative every tick (using fresh world state directly)
      await refreshNarrative(response.world_state);
    } catch (err) {
      setError(`Failed to advance tick: ${err}`);
    } finally {
      setIsLoading(false);
    }
  }, [isLoading, isGeneratingNarrative, refreshState, refreshNarrative]);

  // Handle action submit
  const handleActionSubmit = useCallback(async (actionId: string) => {
    console.log('[App] handleActionSubmit called with actionId:', actionId);
    if (isLoading) {
      console.log('[App] Skipping - already loading');
      return;
    }

    try {
      setIsLoading(true);
      setError(null);
      console.log('[App] Calling submitAction API with:', actionId);
      const response = await submitAction(actionId);
      console.log('[App] submitAction response:', response);
      setWorldState(response.new_state);
      await refreshState();
    } catch (err) {
      console.error('[App] submitAction error:', err);
      setError(`Failed to execute action: ${err}`);
    } finally {
      setIsLoading(false);
    }
  }, [isLoading, refreshState]);

  // Handle save game - opens modal
  const handleOpenSaveModal = useCallback(() => {
    setShowSaveModal(true);
    // Fetch current saves for this scenario
    if (worldState) {
      listSavesWithSlots(worldState.scenario_id)
        .then(setCurrentSaves)
        .catch(err => console.error('Failed to fetch saves:', err));
    }
  }, [worldState]);

  // Handle actual save to slot
  const handleSaveToSlot = useCallback(async (slot: string) => {
    try {
      const response = await saveGame(slot);
      if (response.success) {
        setSaveNotification(`Сохранено в ${slot === 'auto' ? 'автослот' : slot}`);
        setTimeout(() => setSaveNotification(null), 2000);
      } else {
        setError(response.error || 'Failed to save');
      }
    } catch (err) {
      setError(`Failed to save: ${err}`);
    }
  }, []);

  // Save notification state
  const [saveNotification, setSaveNotification] = useState<string | null>(null);
  
  // Save modal state
  const [showSaveModal, setShowSaveModal] = useState(false);
  const [currentSaves, setCurrentSaves] = useState<SaveSlotList | null>(null);

  // Render menu screen
  if (gameState === 'menu') {
    return (
      <ScenarioSelectScreen
        scenarios={scenarios}
        hasSaves={hasSaves}
        onStartScenario={handleStartScenario}
        onContinue={handleContinue}
      />
    );
  }

  // Loading state
  if (!scenarioLoaded) {
    return (
      <div className="app loading">
        <div className="loading-screen">
          <h1>ENGINE13</h1>
          <p>{loadingStep}</p>
          {error && <p className="error">{error}</p>}
        </div>
      </div>
    );
  }

  if (!worldState) {
    return (
      <div className="app loading">
        <div className="loading-screen">
          <h1>ENGINE13</h1>
          <p>Initializing...</p>
        </div>
      </div>
    );
  }

  // Render game screen
  return (
    <div className="app">
      <header className="app-header">
        <div className="header-left">
          <h1 className="app-title">ENGINE13</h1>
          <span className="app-subtitle">
            {(() => {
              const scenario = scenarios.find(s => s.id === worldState.scenario_id);
              return scenario?.label ?? worldState.scenario_id;
            })()}
          </span>
        </div>
        <button
          className="settings-button"
          onClick={() => setIsSettingsOpen(true)}
          title="LLM Settings"
        >
          ⚙
        </button>
      </header>

      {error && (
        <div className="error-banner">
          {error}
          <button onClick={() => setError(null)} className="error-dismiss">×</button>
        </div>
      )}

      {statusIndicators.length > 0 && (
        <StatusPanel indicators={statusIndicators} />
      )}

      {worldState?.victory_achieved && (
        <VictoryScreen
          worldState={worldState}
          victoryTitle={(() => {
            const scenario = scenarios.find(s => s.id === worldState.scenario_id);
            return scenario?.victory_title ?? 'Победа!';
          })()}
          victoryDescription={(() => {
            const scenario = scenarios.find(s => s.id === worldState.scenario_id);
            return scenario?.victory_description ?? 'Вы достигли цели сценария.';
          })()}
          onContinue={() => {}}
          onNewGame={() => setGameState('menu')}
        />
      )}

      <main className="app-main">
        <div className="panel-column left-column">
          <WorldPanel
            actors={Object.values(worldState.actors)}
            selectedActorId={selectedActorId}
            onSelectActor={setSelectedActorId}
          />
        </div>

        <div className="panel-column middle-column">
          <NarrativePanel
            narrative={narrative}
            isLoading={narrativeLoading}
          />
          {worldState.features?.family_panel && (
            <FamilyPanel
              worldState={worldState}
              currentYear={worldState.year}
              currentTick={worldState.tick}
            />
          )}
          {worldState.features?.global_metrics_panel && (
            <GlobalMetricsPanel worldState={worldState} metricsDisplay={worldState.global_metrics_display || []} />
          )}
        </div>

        <div className="panel-column right-column">
          <ControlPanel
            currentYear={worldState.year}
            currentTick={worldState.tick}
            worldState={worldState}
            availableActions={availableActions}
            recentEvents={recentEvents}
            onAdvanceTick={handleAdvanceTick}
            onActionSubmit={handleActionSubmit}
            onSaveGame={handleOpenSaveModal}
            isLoading={isLoading || isGeneratingNarrative}
          />
          {worldState.features?.patron_actions && (
            <ActionHistory tick={worldState.tick} />
          )}
        </div>
      </main>

      {saveNotification && (
        <div className="save-notification">
          {saveNotification}
        </div>
      )}

      <SaveSlotModal
        isOpen={showSaveModal}
        onClose={() => setShowSaveModal(false)}
        onSave={handleSaveToSlot}
        saves={currentSaves}
      />

      <SettingsPanel
        isOpen={isSettingsOpen}
        onClose={() => setIsSettingsOpen(false)}
      />
    </div>
  );
};

export default App;
