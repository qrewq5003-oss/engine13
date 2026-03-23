import React, { useState, useEffect } from 'react';
import type { Actor, WorldState } from '../types';
import ActorDetailPanel from './ActorDetailPanel';
import './WorldPanel.css';

interface WorldPanelProps {
  actors: Actor[];
  selectedActorId: string | null;
  onSelectActor: (id: string) => void;
  prevWorldState: WorldState | null;
}

const STORAGE_KEY = 'engine13_pinned_actors';

export const WorldPanel: React.FC<WorldPanelProps> = ({
  actors,
  selectedActorId,
  onSelectActor,
  prevWorldState,
}) => {
  const [detailActor, setDetailActor] = useState<Actor | null>(null);
  const [pinnedActors, setPinnedActors] = useState<Set<string>>(new Set());

  // Load pinned actors from localStorage on mount
  useEffect(() => {
    try {
      const stored = localStorage.getItem(STORAGE_KEY);
      if (stored) {
        const pinned: string[] = JSON.parse(stored);
        setPinnedActors(new Set(pinned));
      }
    } catch (e) {
      console.error('Failed to load pinned actors:', e);
    }
  }, []);

  // Save pinned actors to localStorage when changed
  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(Array.from(pinnedActors)));
    } catch (e) {
      console.error('Failed to save pinned actors:', e);
    }
  }, [pinnedActors]);

  const togglePin = (actorId: string, e: React.MouseEvent) => {
    e.stopPropagation();
    setPinnedActors(prev => {
      const next = new Set(prev);
      if (next.has(actorId)) {
        next.delete(actorId);
      } else {
        next.add(actorId);
      }
      return next;
    });
  };

  // Sort actors: pinned first, then foreground, then by name
  const sortedActors = [...actors].sort((a, b) => {
    const aPinned = pinnedActors.has(a.id);
    const bPinned = pinnedActors.has(b.id);
    if (aPinned && !bPinned) return -1;
    if (!aPinned && bPinned) return 1;
    if (a.narrative_status === 'foreground' && b.narrative_status === 'background') return -1;
    if (a.narrative_status === 'background' && b.narrative_status === 'foreground') return 1;
    return a.name.localeCompare(b.name);
  });

  return (
    <div className="world-panel">
      <h2 className="panel-title">World Actors</h2>
      <div className="actor-list">
        {sortedActors.map((actor) => (
          <div
            key={actor.id}
            className={`actor-card ${actor.narrative_status} ${
              selectedActorId === actor.id ? 'selected' : ''
            }`}
            onClick={() => {
              onSelectActor(actor.id);
              setDetailActor(actor);
            }}
            style={{ cursor: 'pointer' }}
          >
            <div className="actor-header">
              <span className="actor-name">{actor.name_short}</span>
              <div className="actor-actions">
                <button
                  className={`pin-button ${pinnedActors.has(actor.id) ? 'pinned' : ''}`}
                  onClick={(e) => togglePin(actor.id, e)}
                  title={pinnedActors.has(actor.id) ? 'Unpin actor' : 'Pin actor'}
                >
                  📌
                </button>
              </div>
              <span className={`actor-status ${actor.narrative_status}`}>
                {actor.narrative_status === 'foreground' ? '●' : '○'}
              </span>
            </div>
            <div className="actor-region">{actor.region}</div>
            <div className="actor-metrics">
              <MetricBar
                label="Legitimacy"
                value={actor.metrics.legitimacy}
                color="#4caf50"
                prevValue={prevWorldState?.actors[actor.id]?.metrics.legitimacy}
              />
              <MetricBar
                label="Cohesion"
                value={actor.metrics.cohesion}
                color="#2196f3"
                prevValue={prevWorldState?.actors[actor.id]?.metrics.cohesion}
              />
              <MetricBar
                label="Military"
                value={actor.metrics.military_size}
                max={500}
                color="#f44336"
                prevValue={prevWorldState?.actors[actor.id]?.metrics.military_size}
              />
              <MetricBar
                label="Economy"
                value={actor.metrics.economic_output}
                color="#ff9800"
                prevValue={prevWorldState?.actors[actor.id]?.metrics.economic_output}
              />
            </div>
          </div>
        ))}
      </div>

      {detailActor && (
        <ActorDetailPanel
          actor={detailActor}
          onClose={() => setDetailActor(null)}
        />
      )}
    </div>
  );
};

interface MetricBarProps {
  label: string;
  value: number;
  max?: number;
  color: string;
  prevValue?: number;
}

const MetricBar: React.FC<MetricBarProps> = ({ label, value, max = 100, color, prevValue }) => {
  const percentage = Math.min(100, Math.max(0, (value / max) * 100));
  
  // Compute delta only if prevValue exists (actor existed in previous state)
  const delta = prevValue !== undefined ? value - prevValue : undefined;
  const deltaColor = delta !== undefined ? (delta > 0 ? '#4caf50' : delta < 0 ? '#f44336' : undefined) : undefined;
  const deltaSign = delta !== undefined ? (delta > 0 ? '+' : delta < 0 ? '' : '') : undefined;
  const deltaAbs = delta !== undefined ? Math.abs(delta) : undefined;
  
  // Format delta: use integer for metrics displayed as integers
  const formatDelta = (d: number | undefined): string => {
    if (d === undefined) return '';
    // For military_size and economic_output (larger values), show as integer
    // For legitimacy and cohesion (0-100), show as integer
    return d.toFixed(0);
  };

  return (
    <div className="metric-bar">
      <span className="metric-label">{label}</span>
      <div className="metric-fill-container">
        <div
          className="metric-fill"
          style={{ width: `${percentage}%`, backgroundColor: color }}
        />
      </div>
      <div className="metric-value-wrapper">
        <span className="metric-value">{value.toFixed(0)}</span>
        {delta !== undefined && delta !== 0 && (
          <span 
            className="metric-delta" 
            style={{ color: deltaColor }}
          >
            {deltaSign}{formatDelta(deltaAbs)}
          </span>
        )}
      </div>
    </div>
  );
};

export default WorldPanel;
