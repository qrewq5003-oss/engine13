import React from 'react';
import type { StatusIndicatorState } from '../types';
import './StatusPanel.css';

interface StatusPanelProps {
  indicators: StatusIndicatorState[];
}

export const StatusPanel: React.FC<StatusPanelProps> = ({ indicators }) => {
  if (indicators.length === 0) {
    return null;
  }

  return (
    <div className="status-panel">
      <div className="status-cards">
        {indicators.map((indicator, index) => {
          const colorClass = getStatusColor(indicator.progress, indicator.invert);
          return (
            <div key={index} className="status-card">
              <div className="status-header">
                <span className="status-label">{indicator.label}</span>
              </div>
              <div className="status-value">{indicator.status_text}</div>
              <div className="status-bar-container">
                <div
                  className={`status-bar-fill ${colorClass}`}
                  style={{ width: `${indicator.progress * 100}%` }}
                />
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};

function getStatusColor(progress: number, invert: boolean = false): string {
  const effectiveProgress = invert ? 1.0 - progress : progress;

  if (effectiveProgress < 0.33) {
    return 'status-green';
  } else if (effectiveProgress < 0.66) {
    return 'status-yellow';
  } else {
    return 'status-red';
  }
}

export default StatusPanel;
