import React, { useState, useEffect, useCallback } from 'react';
import { WorldPanel } from './components/WorldPanel';
import { FamilyPanel } from './components/FamilyPanel';
import { ControlPanel } from './components/ControlPanel';
import { NarrativePanel } from './components/NarrativePanel';
import { SettingsPanel } from './components/SettingsPanel';
import {
  loadScenario,
  getWorldState,
  getAvailableActions,
  advanceTick,
  submitAction,
  getRelevantEvents,
  getNarrative,
} from './api';
import type { WorldState, Actor, PatronAction, Event } from './types';
import './App.css';

const App: React.FC = () => {
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
  const [lastNarrativeTick, setLastNarrativeTick] = useState<number>(-5);
  
  // Settings state
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);

  // Load scenario on mount
  useEffect(() => {
    const initScenario = async () => {
      try {
        console.log('[App] Starting scenario initialization');
        setIsLoading(true);
        setLoadingStep('Loading scenario...');

        console.log('[App] Calling loadScenario');
        const loadResult = await loadScenario('rome_375');
        console.log('[App] loadScenario result:', loadResult);

        if (!loadResult.success) {
          throw new Error(loadResult.error || 'Failed to load scenario');
        }

        setLoadingStep('Fetching world state...');
        await refreshState();
        await refreshNarrative();

        setLoadingStep('Complete');
        setScenarioLoaded(true);
      } catch (err) {
        console.error('[App] Initialization error:', err);
        setError(`Failed to load scenario: ${err}`);
      } finally {
        setIsLoading(false);
      }
    };

    initScenario();
  }, []);

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

  // Refresh narrative
  const refreshNarrative = useCallback(async () => {
    try {
      setNarrativeLoading(true);
      setIsGeneratingNarrative(true);
      const result = await getNarrative();
      console.log('[Narrative] Got text:', result?.slice(0, 50));
      console.log('[Narrative] Full result type:', typeof result, 'length:', result?.length);
      console.log('[Narrative] Result is empty:', !result);
      console.log('[Narrative] Result value:', JSON.stringify(result));

      // Handle empty string case - use placeholder
      if (!result || result.trim() === '') {
        const placeholder = worldState
          ? `Медиолан, ${worldState.year} год. Семья наблюдает за судьбой Империи.`
          : 'Медиолан. Семья наблюдает за судьбой Империи.';
        console.log('[Narrative] Using placeholder because result was empty');
        setNarrative(placeholder);
      } else {
        setNarrative(result);
      }
    } catch (err) {
      console.error('[Narrative] Error:', err);
      // Don't show error - LLM may be unavailable, use placeholder
      const placeholder = worldState
        ? `Медиолан, ${worldState.year} год. Семья наблюдает за судьбой Империи.`
        : 'Медиолан. Семья наблюдает за судьбой Империи.';
      setNarrative(placeholder);
    } finally {
      setNarrativeLoading(false);
      setIsGeneratingNarrative(false);
    }
  }, [worldState]);

  // Auto-refresh narrative every 5 ticks
  useEffect(() => {
    if (worldState && scenarioLoaded) {
      const ticksSinceLastNarrative = worldState.tick - lastNarrativeTick;
      if (ticksSinceLastNarrative >= 5) {
        const fetchNarrative = async () => {
          try {
            setNarrativeLoading(true);
            setIsGeneratingNarrative(true);
            const result = await getNarrative();
            console.log('[Narrative] Auto-refresh Got text:', result?.slice(0, 50));
            console.log('[Narrative] Auto-refresh Result value:', JSON.stringify(result));

            // Handle empty string case
            if (!result || result.trim() === '') {
              const placeholder = `Медиолан, ${worldState.year} год. Семья наблюдает за судьбой Империи.`;
              console.log('[Narrative] Auto-refresh Using placeholder');
              setNarrative(placeholder);
            } else {
              setNarrative(result);
            }
          } catch (err) {
            console.error('[Narrative] Auto-refresh Error:', err);
            const placeholder = `Медиолан, ${worldState.year} год. Семья наблюдает за судьбой Империи.`;
            setNarrative(placeholder);
          } finally {
            setNarrativeLoading(false);
            setIsGeneratingNarrative(false);
          }
        };
        fetchNarrative();
        setLastNarrativeTick(worldState.tick);
      }
    }
  }, [worldState?.tick, worldState?.year, scenarioLoaded, lastNarrativeTick]);

  // Handle advance tick
  const handleAdvanceTick = useCallback(async () => {
    if (isLoading) return;

    try {
      setIsLoading(true);
      setError(null);
      const response = await advanceTick();
      setWorldState(response.world_state);
      if (response.events.length > 0) {
        setRecentEvents(response.events);
      }
      // Refresh available actions and events to reflect new state
      await refreshState();
    } catch (err) {
      setError(`Failed to advance tick: ${err}`);
    } finally {
      setIsLoading(false);
    }
  }, [isLoading, refreshState]);

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

  return (
    <div className="app">
      <header className="app-header">
        <div className="header-left">
          <h1 className="app-title">ENGINE13</h1>
          <span className="app-subtitle">Rome 375 — Family Di Milano</span>
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
          <FamilyPanel
            worldState={worldState}
            currentYear={worldState.year}
            currentTick={worldState.tick}
          />
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
