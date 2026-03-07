import React from 'react';
import type { Actor } from '../types';
import './ActorDetailPanel.css';

interface ActorDetailPanelProps {
  actor: Actor;
  onClose: () => void;
}

export const ActorDetailPanel: React.FC<ActorDetailPanelProps> = ({ actor, onClose }) => {
  const getPercentage = (value: number, max: number = 100) => {
    return Math.min(100, Math.max(0, (value / max) * 100));
  };

  const renderMetricBar = (label: string, value: number, max: number = 100, color: string) => (
    <div className="detail-metric-row">
      <span className="detail-metric-label">{label}</span>
      <span className="detail-metric-value">{value.toFixed(0)}</span>
      <div className="detail-metric-fill-container">
        <div
          className="detail-metric-fill"
          style={{ width: `${getPercentage(value, max)}%`, backgroundColor: color }}
        />
      </div>
    </div>
  );

  const hasScenarioMetrics = Object.keys(actor.scenario_metrics).length > 0;
  const hasTags = actor.tags.length > 0;

  return (
    <div className="actor-detail-overlay" onClick={onClose}>
      <div className="actor-detail-panel" onClick={(e) => e.stopPropagation()}>
        <div className="detail-header">
          <h2 className="detail-title">{actor.name}</h2>
          <button className="detail-close" onClick={onClose}>×</button>
        </div>

        <div className="detail-info-row">
          <span className="detail-info-label">Region:</span>
          <span className="detail-info-value">{actor.region}</span>
          <span className="detail-info-label">Status:</span>
          <span className={`detail-status ${actor.narrative_status}`}>
            {actor.narrative_status}
          </span>
        </div>

        <div className="detail-section">
          <h3 className="detail-section-title">Метрики</h3>
          
          {renderMetricBar('Legitimacy', actor.metrics.legitimacy, 100, '#4caf50')}
          {renderMetricBar('Cohesion', actor.metrics.cohesion, 100, '#2196f3')}
          {renderMetricBar('Military Size', actor.metrics.military_size, 500, '#f44336')}
          {renderMetricBar('Military Quality', actor.metrics.military_quality, 100, '#9c27b0')}
          {renderMetricBar('Economy', actor.metrics.economic_output, 100, '#ff9800')}
          {renderMetricBar('Ext. Pressure', actor.metrics.external_pressure, 100, '#795548')}
        </div>

        <div className="detail-section">
          <h3 className="detail-section-title">Ресурсы</h3>
          
          <div className="detail-resource-row">
            <span className="detail-resource-label">Treasury</span>
            <span className="detail-resource-value">{actor.metrics.treasury.toFixed(0)}</span>
          </div>
          
          <div className="detail-resource-row">
            <span className="detail-resource-label">Population</span>
            <span className="detail-resource-value">{actor.metrics.population.toFixed(0)}</span>
          </div>
        </div>

        {hasScenarioMetrics && (
          <div className="detail-section">
            <h3 className="detail-section-title">Доп. метрики</h3>
            {Object.entries(actor.scenario_metrics).map(([key, value]) => (
              <div key={key} className="detail-scenario-metric">
                <span className="detail-scenario-key">{key}</span>
                <span className="detail-scenario-value">{value.toFixed(0)}</span>
              </div>
            ))}
          </div>
        )}

        {hasTags && (
          <div className="detail-section">
            <h3 className="detail-section-title">Теги</h3>
            <div className="detail-tags">
              {actor.tags.map((tag) => (
                <span key={tag} className="detail-tag">{tag}</span>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
};

export default ActorDetailPanel;
