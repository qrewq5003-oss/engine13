import React from 'react';
import type { WorldState } from '../types';
import './GlobalMetricsPanel.css';

interface GlobalMetricsPanelProps {
  worldState: WorldState;
}

export const GlobalMetricsPanel: React.FC<GlobalMetricsPanelProps> = ({ worldState }) => {
  const federationProgress = worldState.global_metrics?.federation_progress ?? 0;

  return (
    <div className="global-metrics-panel">
      <h2 className="panel-title">Федерация</h2>

      <div className="global-metric">
        <div className="global-metric-header">
          <span className="global-metric-label">Прогресс федерации</span>
          <span className="global-metric-value">{Math.round(federationProgress)}%</span>
        </div>
        <div className="global-metric-fill-container">
          <div
            className="global-metric-fill"
            style={{ width: `${Math.min(100, Math.max(0, federationProgress))}%` }}
          />
        </div>
        <span className="global-metric-description">
          {getFederationStatus(federationProgress)}
        </span>
      </div>
    </div>
  );
};

function getFederationStatus(progress: number): string {
  if (progress < 20) {
    return 'Разговоры ни к чему не обязывающие';
  } else if (progress < 50) {
    return 'Первые договорённости, взаимное недоверие';
  } else if (progress < 80) {
    return 'Реальный союз, совместные действия';
  } else {
    return 'Федерация — исторически беспрецедентное событие';
  }
}

export default GlobalMetricsPanel;
