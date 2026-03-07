import React, { useState, useEffect } from 'react';
import { listSavesWithSlots, loadGame } from '../api';
import type { ScenarioMeta, SaveSlotData, SaveSlotList } from '../types';
import './ScenarioSelectScreen.css';

interface ScenarioSelectScreenProps {
  scenarios: ScenarioMeta[];
  hasSaves: boolean;
  onStartScenario: (scenarioId: string) => void;
  onContinue: (saves: SaveSlotData[]) => void;
}

export const ScenarioSelectScreen: React.FC<ScenarioSelectScreenProps> = ({
  scenarios,
  hasSaves,
  onStartScenario,
  onContinue,
}) => {
  const [savesByScenario, setSavesByScenario] = useState<Record<string, SaveSlotList>>({});
  const [expandedScenario, setExpandedScenario] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  useEffect(() => {
    const fetchSaves = async () => {
      const savesMap: Record<string, SaveSlotList> = {};
      for (const scenario of scenarios) {
        try {
          const slotList = await listSavesWithSlots(scenario.id);
          savesMap[scenario.id] = slotList;
        } catch (err) {
          console.error(`Failed to fetch saves for ${scenario.id}:`, err);
        }
      }
      setSavesByScenario(savesMap);
    };
    fetchSaves();
  }, [scenarios]);

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
        window.location.reload();
      }
    } catch (err) {
      console.error('Failed to load game:', err);
    } finally {
      setIsLoading(false);
    }
  };

  const handleContinue = async () => {
    const allSaves: SaveSlotData[] = [];
    Object.values(savesByScenario).forEach(slotList => {
      if (slotList.auto) allSaves.push(slotList.auto);
      slotList.slots.forEach(slot => { if (slot) allSaves.push(slot); });
    });
    onContinue(allSaves);
  };

  return (
    <div className="scenario-select-screen">
      <div className="scenario-select-content">
        <h1 className="scenario-title">ENGINE13</h1>
        <p className="scenario-subtitle">Выбор сценария</p>

        <div className="scenario-list">
          {scenarios.map((scenario) => {
            const slotList = savesByScenario[scenario.id];
            const hasAutoSave = slotList?.auto !== null && slotList?.auto !== undefined;
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
                  
                  {hasAutoSave && (
                    <button
                      className="scenario-continue-button"
                      onClick={() => handleLoadGame(slotList.auto!.id)}
                      disabled={isLoading}
                    >
                      Продолжить ({slotList.auto!.year} AD)
                    </button>
                  )}
                  
                  <button
                    className="scenario-loadsaves-button"
                    onClick={() => setExpandedScenario(isExpanded ? null : scenario.id)}
                    disabled={isLoading}
                  >
                    Загрузить →
                  </button>
                </div>

                {isExpanded && (
                  <div className="saves-list">
                    {slotList?.slots.map((slot, index) => (
                      <div key={`slot_${index + 1}`} className="save-item">
                        {slot ? (
                          <>
                            <div className="save-info">
                              <span className="save-year">{slot.year} AD</span>
                              <span className="save-date">{formatDateTime(slot.created_at)}</span>
                              <span className="save-tick">Тик: {slot.tick}</span>
                            </div>
                            <button
                              className="save-load-button"
                              onClick={() => handleLoadGame(slot.id)}
                              disabled={isLoading}
                            >
                              Загрузить
                            </button>
                          </>
                        ) : (
                          <div className="save-empty">
                            <span>Слот {index + 1}: Пусто</span>
                          </div>
                        )}
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
