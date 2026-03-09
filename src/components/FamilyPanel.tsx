import React from 'react';
import type { WorldState } from '../types';
import './FamilyPanel.css';

interface FamilyPanelProps {
  worldState: WorldState;
  currentYear: number;
  currentTick: number;
}

export const FamilyPanel: React.FC<FamilyPanelProps> = ({
  worldState,
  currentYear,
  currentTick: _currentTick,
}) => {
  // Use family_state if available
  const familyState = worldState.family_state;
  const genMechanics = worldState.generation_mechanics;
  
  if (!familyState || !genMechanics) {
    return <div className="family-panel">No family data available</div>;
  }

  // Dynamically render all family metrics from family_state
  const familyMetrics = Object.entries(familyState.metrics).map(([key, value]) => ({
    key,
    label: key.replace('family_', '').replace(/_/g, ' ').replace(/\b\w/g, l => l.toUpperCase()),
    value: value as number,
  }));

  // Get patriarch age from family_state
  const patriarchAge = familyState.patriarch_age;

  // Calculate generation info - use scenario start year from worldState
  const startYear = worldState.scenario_start_year ?? currentYear;
  const genLength = genMechanics.generation_length;
  const yearsSinceStart = currentYear - startYear;
  const generationNumber = Math.floor(yearsSinceStart / genLength) + 1;
  const generationStartYear = startYear + (generationNumber - 1) * genLength;

  // Get panel label from generation_mechanics
  const panelLabel = genMechanics.panel_label;

  // Get era text from generation_mechanics
  const eraText = genMechanics.era_texts.find(e => currentYear >= e.from_year && currentYear < e.to_year)?.text
    ?? genMechanics.era_texts[genMechanics.era_texts.length - 1]?.text
    ?? '';

  return (
    <div className="family-panel">
      <h2 className="panel-title">{panelLabel}</h2>

      <div className="generation-info">
        <div className="generation-header">
          <span className="generation-label">Поколение</span>
          <span className="generation-number">{generationNumber}</span>
        </div>
        <div className="patriarch-info">
          <span className="patriarch-age">Возраст: {patriarchAge}</span>
          <span className="generation-year">С {generationStartYear}</span>
        </div>
      </div>

      <div className="family-metrics">
        {familyMetrics.map((metric) => (
          <FamilyMetricBar
            key={metric.key}
            label={metric.label}
            value={metric.value}
            description={`${metric.label} metric`}
            color="#89b4fa"
          />
        ))}
      </div>

      <div className="family-context">
        <p className="context-text">{eraText}</p>
      </div>
    </div>
  );
};

interface FamilyMetricBarProps {
  label: string;
  value: number;
  description: string;
  color: string;
}

const FamilyMetricBar: React.FC<FamilyMetricBarProps> = ({
  label,
  value,
  description,
  color,
}) => {
  const percentage = Math.min(100, Math.max(0, value));

  return (
    <div className="family-metric">
      <div className="family-metric-header">
        <span className="family-metric-label">{label}</span>
        <span className="family-metric-value">{value.toFixed(0)}</span>
      </div>
      <div className="family-metric-fill-container">
        <div
          className="family-metric-fill"
          style={{ width: `${percentage}%`, backgroundColor: color }}
        />
      </div>
      <span className="family-metric-description">{description}</span>
    </div>
  );
};

export default FamilyPanel;
