import React, { useState } from 'react';
import type { Actor } from '../types';
import ActorDetailPanel from './ActorDetailPanel';
import './WorldPanel.css';

interface WorldPanelProps {
  actors: Actor[];
  selectedActorId: string | null;
  onSelectActor: (id: string) => void;
}

export const WorldPanel: React.FC<WorldPanelProps> = ({
  actors,
  selectedActorId,
  onSelectActor,
}) => {
  const [detailActor, setDetailActor] = useState<Actor | null>(null);

  // Sort actors: foreground first, then by name
  const sortedActors = [...actors].sort((a, b) => {
    if (a.narrative_status === 'foreground' && b.narrative_status === 'background') {
      return -1;
    }
    if (a.narrative_status === 'background' && b.narrative_status === 'foreground') {
      return 1;
    }
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
              />
              <MetricBar
                label="Cohesion"
                value={actor.metrics.cohesion}
                color="#2196f3"
              />
              <MetricBar
                label="Military"
                value={actor.metrics.military_size}
                max={500}
                color="#f44336"
              />
              <MetricBar
                label="Economy"
                value={actor.metrics.economic_output}
                color="#ff9800"
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
}

const MetricBar: React.FC<MetricBarProps> = ({ label, value, max = 100, color }) => {
  const percentage = Math.min(100, Math.max(0, (value / max) * 100));

  return (
    <div className="metric-bar">
      <span className="metric-label">{label}</span>
      <div className="metric-fill-container">
        <div
          className="metric-fill"
          style={{ width: `${percentage}%`, backgroundColor: color }}
        />
      </div>
      <span className="metric-value">{value.toFixed(0)}</span>
    </div>
  );
};

export default WorldPanel;
