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
  // Dynamically render all family_* metrics from global_metrics
  const familyMetrics = Object.entries(worldState.global_metrics || {})
    .filter(([key]) => key.startsWith('family_'))
    .map(([key, value]) => ({
      key,
      label: key.replace('family_', '').replace(/_/g, ' ').replace(/\b\w/g, l => l.toUpperCase()),
      value: value as number,
    }));

  // Get patriarch age from global_metrics with default
  const patriarchAge = Math.floor(worldState.global_metrics?.patriarch_age || 42);

  // Calculate generation info - use scenario start year from global_metrics or default
  const startYear = (worldState.global_metrics?.scenario_start_year as number) || 375;
  const yearsSinceStart = currentYear - startYear;
  const generationLength = 33;
  const generationNumber = Math.floor(yearsSinceStart / generationLength) + 1;
  const generationStartYear = startYear + (generationNumber - 1) * generationLength;

  // Use generic label (scenario-specific labels would require API changes)
  const scenarioLabel = 'Family';

  return (
    <div className="family-panel">
      <h2 className="panel-title">{scenarioLabel}</h2>

      <div className="generation-info">
        <div className="generation-header">
          <span className="generation-label">Generation</span>
          <span className="generation-number">{generationNumber}</span>
        </div>
        <div className="patriarch-info">
          <span className="patriarch-age">Age: {patriarchAge}</span>
          <span className="generation-year">Since {generationStartYear}</span>
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
        <p className="context-text">
          {getContextText(currentYear, scenarioLabel)}
        </p>
      </div>
    </div>
  );
};

function getContextText(year: number, scenarioLabel: string): string {
  // Generic context text based on era
  if (year < 400) {
    return `${scenarioLabel}, ${year} AD. The old order crumbles as new powers rise.`;
  } else if (year <= 500) {
    return `${scenarioLabel}, ${year} AD. Kingdoms carve their realms from the ashes of empire.`;
  } else {
    return `${scenarioLabel}, ${year} AD. New powers rise from the ashes of civilization.`;
  }
}

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
