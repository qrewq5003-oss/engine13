import React from 'react';
import { listSaves } from '../api';
import type { ScenarioMeta, SaveData } from '../types';
import './ScenarioSelectScreen.css';

interface ScenarioSelectScreenProps {
  scenarios: ScenarioMeta[];
  hasSaves: boolean;
  onStartScenario: (scenarioId: string) => void;
  onContinue: (saves: SaveData[]) => void;
}

export const ScenarioSelectScreen: React.FC<ScenarioSelectScreenProps> = ({
  scenarios,
  hasSaves,
  onStartScenario,
  onContinue,
}) => {
  const handleContinue = async () => {
    const saves = await listSaves();
    onContinue(saves);
  };

  return (
    <div className="scenario-select-screen">
      <div className="scenario-select-content">
        <h1 className="scenario-title">ENGINE13</h1>
        <p className="scenario-subtitle">Выбор сценария</p>

        <div className="scenario-list">
          {scenarios.map((scenario) => (
            <div key={scenario.id} className="scenario-card">
              <div className="scenario-card-header">
                <h2 className="scenario-name">{scenario.label}</h2>
                <span className="scenario-year">{scenario.start_year}</span>
              </div>
              <p className="scenario-description">{scenario.description}</p>
              <button
                className="scenario-start-button"
                onClick={() => onStartScenario(scenario.id)}
              >
                Начать
              </button>
            </div>
          ))}
        </div>

        {hasSaves && (
          <div className="continue-section">
            <button
              className="continue-button"
              onClick={handleContinue}
            >
              Продолжить
            </button>
          </div>
        )}
      </div>
    </div>
  );
};
