import React, { useState, useEffect, useCallback } from 'react';
import { WorldPanel } from './components/WorldPanel';
import { FamilyPanel } from './components/FamilyPanel';
import { ControlPanel } from './components/ControlPanel';
import {
  loadScenario,
  getWorldState,
  getAvailableActions,
  advanceTick,
  submitAction,
  getRelevantEvents,
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
        const narrativeActorIds = Object.values(world.actors)
          .filter(a => a.narrative_status === 'foreground')
          .map(a => a.id);
        const events = await getRelevantEvents(narrativeActorIds, world.tick);
        setRecentEvents(events);
      }

      setAvailableActions(actions || []);
    } catch (err) {
      setError(`Failed to refresh state: ${err}`);
    }
  }, []);

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
    } catch (err) {
      setError(`Failed to advance tick: ${err}`);
    } finally {
      setIsLoading(false);
    }
  }, [isLoading]);

  // Handle action submit
  const handleActionSubmit = useCallback(async (actionId: string) => {
    if (isLoading) return;

    try {
      setIsLoading(true);
      setError(null);
      const response = await submitAction(actionId);
      setWorldState(response.new_state);
      await refreshState();
    } catch (err) {
      setError(`Failed to execute action: ${err}`);
    } finally {
      setIsLoading(false);
    }
  }, [isLoading, refreshState]);

  // Get Rome actor for family panel
  const romeActor = worldState?.actors['rome'] || null;

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
        <h1 className="app-title">ENGINE13</h1>
        <span className="app-subtitle">Rome 375 — Family Di Milano</span>
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
          <FamilyPanel
            romeActor={romeActor}
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
            isLoading={isLoading}
          />
        </div>
      </main>
    </div>
  );
};

export default App;
