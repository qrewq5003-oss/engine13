import React, { useState, useEffect, useCallback } from 'react';
import { WorldPanel } from './components/WorldPanel';
import { FamilyPanel } from './components/FamilyPanel';
import { GlobalMetricsPanel } from './components/GlobalMetricsPanel';
import { ControlPanel } from './components/ControlPanel';
import { NarrativePanel } from './components/NarrativePanel';
import { SettingsPanel } from './components/SettingsPanel';
import { ScenarioSelectScreen } from './components/ScenarioSelectScreen';
import {
  loadScenario,
  getWorldState,
  getAvailableActions,
  advanceTick,
  submitAction,
  getRelevantEvents,
  getNarrative,
  getScenarioList,
  listSaves,
  loadGame,
} from './api';
import type { WorldState, Actor, PatronAction, Event, ScenarioMeta, SaveData } from './types';
import './App.css';

const App: React.FC = () => {
  // Game state: 'menu' or 'playing'
  const [gameState, setGameState] = useState<'menu' | 'playing'>('menu');
  
  // Menu state
  const [scenarios, setScenarios] = useState<ScenarioMeta[]>([]);
  const [hasSaves, setHasSaves] = useState(false);
  
  // Game state
  const [worldState, setWorldState] = useState<WorldState | null>(null);
  const [availableActions, setAvailableActions] = useState<PatronAction[]>([]);
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
  const handleContinue = async (saves: SaveData[]) => {
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
        getAvailableActions(),
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
      }

      setAvailableActions(actions || []);
    } catch (err) {
      setError(`Failed to refresh state: ${err}`);
    }
  }, []);

  // Refresh narrative (streaming - uses current world state from API)
  const refreshNarrative = useCallback(async () => {
    try {
      setNarrativeLoading(true);
      setIsGeneratingNarrative(true);
      setNarrative(''); // Reset narrative before streaming
      
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
          const placeholder = worldState
            ? `Медиолан, ${worldState.year} год. Семья наблюдает за судьбой Империи.`
            : 'Медиолан. Семья наблюдает за судьбой Империи.';
          setNarrative(placeholder);
          setNarrativeLoading(false);
          setIsGeneratingNarrative(false);
        }
      );
    } catch (err) {
      console.error('[Narrative] Error:', err);
      // Don't show error - LLM may be unavailable, use placeholder
      const placeholder = worldState
        ? `Медиолан, ${worldState.year} год. Семья наблюдает за судьбой Империи.`
        : 'Медиолан. Семья наблюдает за судьбой Империи.';
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

      // Step 3: Generate narrative every tick (using new world state)
      await refreshNarrative();
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
            {worldState.scenario_id === 'rome_375' ? 'Rome 375 — Семья Ди Милано' :
             worldState.scenario_id === 'constantinople_1430' ? 'Constantinople 1430 — Федерация' :
             worldState.scenario_id}
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
          {worldState.scenario_id === 'rome_375' && (
            <FamilyPanel
              worldState={worldState}
              currentYear={worldState.year}
              currentTick={worldState.tick}
            />
          )}
          {worldState.scenario_id === 'constantinople_1430' && (
            <GlobalMetricsPanel worldState={worldState} />
          )}
        </div>

        <div className="panel-column right-column">
          <ControlPanel
            currentYear={worldState.year}
            currentTick={worldState.tick}
            availableActions={availableActions}
            recentEvents={recentEvents}
            onAdvanceTick={handleAdvanceTick}
            onActionSubmit={handleActionSubmit}
            isLoading={isLoading || isGeneratingNarrative}
          />
        </div>
      </main>

      <SettingsPanel
        isOpen={isSettingsOpen}
        onClose={() => setIsSettingsOpen(false)}
      />
    </div>
  );
};

export default App;
