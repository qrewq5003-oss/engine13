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

// Цвет полосы — это «насколько плохо», а не «насколько велико».
//
// Здесь стояло `invert ? 1 - progress : progress`, и это переворачивало цвет в ОБЕ
// стороны. При `invert: true` (высокое значение — беда: давление на Константинополь,
// османское войско) чем хуже, тем зеленее: `ep = 85`, текст «критическое положение»,
// полоса зелёная во всю ширину. При `invert: false` (высокое — благо: Федерация,
// регентство в Милане) наоборот: `federation_progress = 85`, текст «готова», полоса
// красная. Ширина при этом не инвертируется (`progress * 100%`), так что два куска
// одного виджета противоречили друг другу.
function getStatusColor(progress: number, invert: boolean = false): string {
  const badness = invert ? progress : 1.0 - progress;

  if (badness < 0.33) {
    return 'status-green';
  } else if (badness < 0.66) {
    return 'status-yellow';
  } else {
    return 'status-red';
  }
}

export default StatusPanel;
