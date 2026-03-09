import React from 'react';
import type { WorldState, MetricDisplay } from '../types';
import './GlobalMetricsPanel.css';

interface GlobalMetricsPanelProps {
  worldState: WorldState;
  metricsDisplay: MetricDisplay[];
}

export const GlobalMetricsPanel: React.FC<GlobalMetricsPanelProps> = ({ worldState, metricsDisplay }) => {
  if (metricsDisplay.length === 0) return null;

  return (
    <div className="global-metrics-panel">
      {metricsDisplay.map(md => {
        const metricKey = md.metric.replace('global:', '');
        const value = worldState.global_metrics?.[metricKey] ?? 0;
        const statusText = md.thresholds.find(t => value < t.below)?.text ?? '';
        return (
          <div key={md.metric} className="global-metric">
            <h2 className="panel-title">{md.panel_title}</h2>
            <div className="global-metric-header">
              <span className="global-metric-label">{md.label}</span>
              <span className="global-metric-value">{Math.round(value)}%</span>
            </div>
            <div className="global-metric-fill-container">
              <div
                className="global-metric-fill"
                style={{ width: `${Math.min(100, Math.max(0, value))}%` }}
              />
            </div>
            <span className="global-metric-description">{statusText}</span>
          </div>
        );
      })}
    </div>
  );
};

export default GlobalMetricsPanel;
