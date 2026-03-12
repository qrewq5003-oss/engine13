import React, { useState } from 'react';
import type { Event, WorldState, ActionInfo, UnavailableReason } from '../types';
import './ControlPanel.css';

interface ControlPanelProps {
  currentYear: number;
  currentTick: number;
  worldState: WorldState;
  availableActions: ActionInfo[];
  recentEvents: Event[];
  onAdvanceTick: () => void;
  onActionSubmit: (actionId: string) => void;
  onSaveGame: () => void;
  isLoading: boolean;
}

export const ControlPanel: React.FC<ControlPanelProps> = ({
  currentYear,
  currentTick,
  worldState,
  availableActions,
  recentEvents,
  onAdvanceTick,
  onActionSubmit,
  onSaveGame,
  isLoading,
}) => {
  const [expandedAction, setExpandedAction] = useState<string | null>(null);

  const handleActionClick = (actionId: string) => {
    if (expandedAction === actionId) {
      setExpandedAction(null);
    } else {
      setExpandedAction(actionId);
    }
  };

  const actionsLimitReached = worldState.actions_per_tick > 0
    && worldState.actions_this_tick >= worldState.actions_per_tick;

  // Sort: available actions first, then unavailable
  const sortedActions = [...availableActions].sort((a, b) =>
    (b.available ? 1 : 0) - (a.available ? 1 : 0)
  );

  // Get reason text for display
  function reasonText(reason: UnavailableReason): string {
    switch (reason.type) {
      case 'InsufficientCost':
        return `Требует ${reason.required} ${reason.resource} (есть: ${reason.available})`;
      case 'ActionsPerTickExhausted':
        return `Лимит действий за тик исчерпан (${reason.limit})`;
      case 'ConditionNotMet':
        return reason.description;
    }
  }

  // Get last 3 events
  const lastThreeEvents = recentEvents.slice(-3).reverse();

  // Compute half-year from tick (even = FirstHalf, odd = SecondHalf)
  const halfYearText = currentTick % 2 === 0 ? 'Первая половина года' : 'Вторая половина года';

  return (
    <div className="control-panel">
      <div className="time-display">
        <span className="year">{currentYear} AD — {halfYearText}</span>
      </div>

      <div className="control-buttons">
        <button
          className="advance-button"
          onClick={onAdvanceTick}
          disabled={isLoading}
        >
          {isLoading ? 'Processing...' : 'Next Tick →'}
        </button>
        <button
          className="save-button"
          onClick={onSaveGame}
          disabled={isLoading}
        >
          Сохранить
        </button>
      </div>

      <div className="actions-section">
        <div className="actions-header">
          <h3 className="section-title">Available Actions</h3>
          {worldState.actions_per_tick > 0 && (
            <div className={`actions-counter ${actionsLimitReached ? 'limit-reached' : ''}`}>
              Действия: {worldState.actions_this_tick} / {worldState.actions_per_tick}
            </div>
          )}
        </div>
        <div className="actions-list">
          {sortedActions.length === 0 ? (
            <div className="no-actions">No actions available</div>
          ) : (
            sortedActions.map((actionInfo) => (
              <div
                key={actionInfo.action.id}
                className={`action-item ${expandedAction === actionInfo.action.id ? 'expanded' : ''} ${!actionInfo.available ? 'unavailable' : ''}`}
                onClick={() => actionInfo.available && handleActionClick(actionInfo.action.id)}
              >
                <div className="action-header">
                  <span className="action-name">{actionInfo.action.name}</span>
                  <span className="action-arrow">
                    {expandedAction === actionInfo.action.id ? '▼' : '▶'}
                  </span>
                </div>
                {expandedAction === actionInfo.action.id && (
                  <div className="action-details">
                    <div className="action-costs">
                      <span className="cost-label">Cost:</span>
                      {Object.entries(actionInfo.action.cost).map(([metric, value]) => (
                        <span key={metric} className="cost-item">
                          {formatMetricName(metric)}: {value > 0 ? '+' : ''}{value.toFixed(0)}
                        </span>
                      ))}
                    </div>
                    <div className="action-effects">
                      <span className="effect-label">Effects:</span>
                      {Object.entries(actionInfo.action.effects).map(([metric, value]) => (
                        <span key={metric} className="effect-item">
                          {formatMetricName(metric)}: +{value.toFixed(0)}
                        </span>
                      ))}
                    </div>
                    <button
                      className="action-submit-button"
                      onClick={(e) => {
                        e.stopPropagation();
                        e.preventDefault();
                        onActionSubmit(actionInfo.action.id);
                      }}
                      disabled={isLoading || actionsLimitReached || !actionInfo.available}
                    >
                      Execute
                    </button>
                    {!actionInfo.available && actionInfo.unavailable_reason && (
                      <span className="action-reason">{reasonText(actionInfo.unavailable_reason)}</span>
                    )}
                  </div>
                )}
              </div>
            ))
          )}
        </div>
      </div>

      <div className="events-section">
        <h3 className="section-title">Recent Events</h3>
        <div className="events-list">
          {lastThreeEvents.length === 0 ? (
            <div className="no-events">No events yet</div>
          ) : (
            lastThreeEvents.map((event) => (
              <div
                key={event.id}
                className={`event-row ${event.is_key ? 'event-key' : ''}`}
              >
                <div className="event-meta">
                  <span className="event-year">{event.year}</span>
                  {event.is_key && <span className="event-key-badge">KEY</span>}
                </div>
                <div className="event-text">{event.description}</div>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
};

function formatMetricName(metric: string): string {
  return metric
    .replace('family_', '')
    .replace('rome.', '')
    .split('_')
    .map(word => word.charAt(0).toUpperCase() + word.slice(1))
    .join(' ');
}

export default ControlPanel;
