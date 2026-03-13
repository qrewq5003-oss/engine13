import React, { useState } from 'react';
import type { WorldState } from '../types';
import { invoke } from '@tauri-apps/api/core';
import './DebugPanel.css';

interface DebugPanelProps {
  worldState: WorldState | null;
  onRefresh: () => Promise<void>;
  isOpen: boolean;
  onClose: () => void;
}

export const DebugPanel: React.FC<DebugPanelProps> = ({ worldState, onRefresh, isOpen, onClose }) => {
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  // Metric editor state
  const [selectedActorId, setSelectedActorId] = useState<string>('');
  const [selectedMetric, setSelectedMetric] = useState<string>('');
  const [metricValue, setMetricValue] = useState<string>('');

  // Force spawn state
  const [spawnActorId, setSpawnActorId] = useState<string>('');
  const [spawnLabel, setSpawnLabel] = useState<string>('');
  const [spawnLat, setSpawnLat] = useState<string>('45.0');
  const [spawnLng, setSpawnLng] = useState<string>('25.0');
  const [spawnMetrics, setSpawnMetrics] = useState({
    cohesion: '50.0',
    legitimacy: '50.0',
    military_size: '50.0',
    economic_output: '50.0',
  });

  // Handle silent tick
  const handleSilentTick = async () => {
    if (isLoading) return;
    setError(null);
    setIsLoading(true);

    try {
      await invoke('cmd_advance_tick_silent');
      await onRefresh();
    } catch (err) {
      setError(`Silent tick failed: ${err}`);
    } finally {
      setIsLoading(false);
    }
  };

  // Handle set metric
  const handleSetMetric = async () => {
    if (!selectedActorId || !selectedMetric || !metricValue) {
      setError('Please fill all fields');
      return;
    }

    if (isLoading) return;
    setError(null);
    setIsLoading(true);

    try {
      await invoke('cmd_set_metric', {
        actorId: selectedActorId,
        metric: selectedMetric,
        value: parseFloat(metricValue),
      });
      await onRefresh();
      setMetricValue('');
    } catch (err) {
      setError(`Set metric failed: ${err}`);
    } finally {
      setIsLoading(false);
    }
  };

  // Handle force spawn
  const handleForceSpawn = async () => {
    if (!spawnActorId || !spawnLabel) {
      setError('Please fill actor_id and label');
      return;
    }

    // Validate actor_id format
    if (!/^[a-z0-9_]+$/.test(spawnActorId)) {
      setError('actor_id must match [a-z0-9_]+');
      return;
    }

    if (isLoading) return;
    setError(null);
    setIsLoading(true);

    try {
      await invoke('cmd_force_spawn', {
        actorId: spawnActorId,
        label: spawnLabel,
        lat: parseFloat(spawnLat),
        lng: parseFloat(spawnLng),
        initialMetrics: {
          cohesion: parseFloat(spawnMetrics.cohesion),
          legitimacy: parseFloat(spawnMetrics.legitimacy),
          military_size: parseFloat(spawnMetrics.military_size),
          economic_output: parseFloat(spawnMetrics.economic_output),
        },
      });
      await onRefresh();
      setSpawnActorId('');
      setSpawnLabel('');
    } catch (err) {
      setError(`Force spawn failed: ${err}`);
    } finally {
      setIsLoading(false);
    }
  };

  // Get selected actor
  const selectedActor = worldState?.actors[selectedActorId];

  // Get metrics for selected actor
  const availableMetrics = selectedActor ? Object.keys(selectedActor.metrics) : [];

  if (!worldState) return null;

  return (
    <div className={`debug-panel ${isOpen ? 'expanded' : 'collapsed'}`}>
      <button
        className="debug-toggle"
        onClick={onClose}
        title="Debug Panel"
      >
        🔧
      </button>

      {isOpen && (
        <div className="debug-content">
          <h3 className="debug-title">Debug / Sandbox</h3>

          {error && (
            <div className="debug-error">
              {error}
              <button onClick={() => setError(null)} className="debug-error-dismiss">×</button>
            </div>
          )}

          {/* Tick Tools */}
          <div className="debug-section">
            <h4 className="debug-section-title">Tick Tools</h4>
            <button
              className="debug-button"
              onClick={handleSilentTick}
              disabled={isLoading}
            >
              ⚡ Тик без LLM
            </button>
          </div>

          {/* Metric Editor */}
          <div className="debug-section">
            <h4 className="debug-section-title">Metric Editor</h4>
            <div className="debug-form">
              <select
                value={selectedActorId}
                onChange={(e) => {
                  setSelectedActorId(e.target.value);
                  setSelectedMetric('');
                }}
                className="debug-select"
              >
                <option value="">Select actor...</option>
                {Object.values(worldState.actors).map((actor) => (
                  <option key={actor.id} value={actor.id}>
                    {actor.name}
                  </option>
                ))}
              </select>

              {selectedActorId && (
                <>
                  <select
                    value={selectedMetric}
                    onChange={(e) => setSelectedMetric(e.target.value)}
                    className="debug-select"
                  >
                    <option value="">Select metric...</option>
                    {availableMetrics.map((metric) => (
                      <option key={metric} value={metric}>
                        {metric}
                      </option>
                    ))}
                  </select>

                  <input
                    type="number"
                    step="0.1"
                    value={metricValue}
                    onChange={(e) => setMetricValue(e.target.value)}
                    placeholder="Value"
                    className="debug-input"
                  />

                  <button
                    className="debug-button debug-button-apply"
                    onClick={handleSetMetric}
                    disabled={isLoading || !selectedMetric || !metricValue}
                  >
                    Apply
                  </button>
                </>
              )}
            </div>
          </div>

          {/* Force Spawn */}
          <div className="debug-section">
            <h4 className="debug-section-title">Force Spawn</h4>
            <div className="debug-form">
              <input
                type="text"
                value={spawnActorId}
                onChange={(e) => setSpawnActorId(e.target.value)}
                placeholder="actor_id [a-z0-9_]+"
                className="debug-input"
              />
              <input
                type="text"
                value={spawnLabel}
                onChange={(e) => setSpawnLabel(e.target.value)}
                placeholder="Label"
                className="debug-input"
              />
              <div className="debug-row">
                <input
                  type="number"
                  step="0.1"
                  value={spawnLat}
                  onChange={(e) => setSpawnLat(e.target.value)}
                  placeholder="Lat"
                  className="debug-input debug-input-small"
                />
                <input
                  type="number"
                  step="0.1"
                  value={spawnLng}
                  onChange={(e) => setSpawnLng(e.target.value)}
                  placeholder="Lng"
                  className="debug-input debug-input-small"
                />
              </div>
              <div className="debug-row">
                <input
                  type="number"
                  step="0.1"
                  value={spawnMetrics.cohesion}
                  onChange={(e) => setSpawnMetrics({ ...spawnMetrics, cohesion: e.target.value })}
                  placeholder="Cohesion"
                  className="debug-input debug-input-small"
                />
                <input
                  type="number"
                  step="0.1"
                  value={spawnMetrics.legitimacy}
                  onChange={(e) => setSpawnMetrics({ ...spawnMetrics, legitimacy: e.target.value })}
                  placeholder="Legitimacy"
                  className="debug-input debug-input-small"
                />
              </div>
              <div className="debug-row">
                <input
                  type="number"
                  step="0.1"
                  value={spawnMetrics.military_size}
                  onChange={(e) => setSpawnMetrics({ ...spawnMetrics, military_size: e.target.value })}
                  placeholder="Military"
                  className="debug-input debug-input-small"
                />
                <input
                  type="number"
                  step="0.1"
                  value={spawnMetrics.economic_output}
                  onChange={(e) => setSpawnMetrics({ ...spawnMetrics, economic_output: e.target.value })}
                  placeholder="Economy"
                  className="debug-input debug-input-small"
                />
              </div>
              <button
                className="debug-button debug-button-spawn"
                onClick={handleForceSpawn}
                disabled={isLoading || !spawnActorId || !spawnLabel}
              >
                Spawn
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

export default DebugPanel;
