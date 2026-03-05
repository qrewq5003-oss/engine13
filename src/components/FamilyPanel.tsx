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
  const familyMetrics = {
    family_influence: worldState.family_metrics.family_influence || 0,
    family_knowledge: worldState.family_metrics.family_knowledge || 0,
    family_wealth: worldState.family_metrics.family_wealth || 0,
    family_connections: worldState.family_metrics.family_connections || 0,
    patriarch_age: worldState.family_metrics.patriarch_age || 42, // Default to 42 if not set
  };

  // Calculate generation info from patriarch_age
  // patriarch_start_age = 42, patriarch_end_age = 75, so generation length ~33 years
  const patriarchAge = Math.floor(familyMetrics.patriarch_age);
  const yearsSinceStart = currentYear - 375;
  const generationLength = 33;
  const generationNumber = Math.floor(yearsSinceStart / generationLength) + 1;
  const generationStartYear = 375 + (generationNumber - 1) * generationLength;

  return (
    <div className="family-panel">
      <h2 className="panel-title">Family Di Milano</h2>
      
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
        <FamilyMetricBar
          label="Influence"
          value={familyMetrics.family_influence}
          description="Political weight in the city"
          color="#f38ba8"
        />
        <FamilyMetricBar
          label="Knowledge"
          value={familyMetrics.family_knowledge}
          description="Accumulated learning, archives"
          color="#89b4fa"
        />
        <FamilyMetricBar
          label="Wealth"
          value={familyMetrics.family_wealth}
          description="Financial base, trade connections"
          color="#fab387"
        />
        <FamilyMetricBar
          label="Connections"
          value={familyMetrics.family_connections}
          description="Network of owed favors"
          color="#a6e3a1"
        />
      </div>

      <div className="family-context">
        <p className="context-text">
          {getContextText(currentYear)}
        </p>
      </div>
    </div>
  );
};

function getContextText(year: number): string {
  if (year < 378) {
    return `Mediolanum, ${year} AD. You are the head of an unnoticed family. The Huns press on the Goths beyond the horizon. The Goths seek refuge across the Danube. Three years until Adrianople — but that has not happened yet.`;
  } else if (year <= 410) {
    return `Mediolanum, ${year} AD. You are the head of an unnoticed family. The Empire reels from Adrianople's disaster. Barbarian kings carve their kingdoms from Roman soil. The old order crumbles.`;
  } else {
    return `Mediolanum, ${year} AD. You are the head of an unnoticed family. The Western Empire has fallen. Rome itself was sacked. In the chaos, new powers rise from the ashes of civilization.`;
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
