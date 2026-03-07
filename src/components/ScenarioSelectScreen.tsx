import React, { useState, useEffect } from 'react';
import { listSaves, loadGame } from '../api';
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
  const [saves, setSaves] = useState<SaveData[]>([]);
  const [expandedScenario, setExpandedScenario] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  useEffect(() => {
    const fetchSaves = async () => {
      const allSaves = await listSaves();
      setSaves(allSaves);
    };
    fetchSaves();
  }, []);

  const getSavesForScenario = (scenarioId: string) => {
    return saves.filter(save => save.scenario_id === scenarioId);
  };

  const formatDateTime = (timestamp: number) => {
    const date = new Date(timestamp * 1000);
    return date.toLocaleDateString('ru-RU', {
      day: 'numeric',
      month: 'long',
      year: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  };

  const handleLoadGame = async (saveId: string) => {
    setIsLoading(true);
    try {
      const result = await loadGame(saveId);
      if (result.success) {
        // Reload the page to refresh state
        window.location.reload();
      }
    } catch (err) {
      console.error('Failed to load game:', err);
    } finally {
      setIsLoading(false);
    }
  };

  const handleContinue = async () => {
    const allSaves = await listSaves();
    onContinue(allSaves);
  };

  return (
    <div className="scenario-select-screen">
      <div className="scenario-select-content">
        <h1 className="scenario-title">ENGINE13</h1>
        <p className="scenario-subtitle">Выбор сценария</p>

        <div className="scenario-list">
          {scenarios.map((scenario) => {
            const scenarioSaves = getSavesForScenario(scenario.id);
            const isExpanded = expandedScenario === scenario.id;

            return (
              <div key={scenario.id} className="scenario-card">
                <div className="scenario-card-header">
                  <h2 className="scenario-name">{scenario.label}</h2>
                  <span className="scenario-year">{scenario.start_year}</span>
                </div>
                <p className="scenario-description">{scenario.description}</p>
                
                <div className="scenario-actions">
                  <button
                    className="scenario-start-button"
                    onClick={() => onStartScenario(scenario.id)}
                    disabled={isLoading}
                  >
                    Начать
                  </button>
                  
                  {scenarioSaves.length > 0 && (
                    <button
                      className="scenario-loadsaves-button"
                      onClick={() => setExpandedScenario(isExpanded ? null : scenario.id)}
                      disabled={isLoading}
                    >
                      Сохранения ({scenarioSaves.length})
                    </button>
                  )}
                </div>

                {isExpanded && scenarioSaves.length > 0 && (
                  <div className="saves-list">
                    {scenarioSaves
                      .sort((a, b) => b.tick - a.tick)
                      .map((save) => (
                        <div key={save.id} className="save-item">
                          <div className="save-info">
                            <span className="save-year">Год: {save.year}</span>
                            <span className="save-date">{formatDateTime(save.created_at)}</span>
                            <span className="save-tick">Тик: {save.tick}</span>
                          </div>
                          <button
                            className="save-load-button"
                            onClick={() => handleLoadGame(save.id)}
                            disabled={isLoading}
                          >
                            Загрузить
                          </button>
                        </div>
                      ))}
                  </div>
                )}
              </div>
            );
          })}
        </div>

        {hasSaves && (
          <div className="continue-section">
            <button
              className="continue-button"
              onClick={handleContinue}
              disabled={isLoading}
            >
              Продолжить (последнее сохранение)
            </button>
          </div>
        )}
      </div>
    </div>
  );
};
