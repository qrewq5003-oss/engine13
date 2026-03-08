import React, { useState } from 'react';
import type { PatronAction, Event, WorldState } from '../types';
import './ControlPanel.css';

interface ControlPanelProps {
  currentYear: number;
  currentTick: number;
  worldState: WorldState;
  availableActions: PatronAction[];
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

  // Get last 3 events
  const lastThreeEvents = recentEvents.slice(-3).reverse();

  return (
    <div className="control-panel">
      <div className="time-display">
        <span className="year">{currentYear} AD</span>
        <span className="tick">Tick {currentTick}</span>
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
          {availableActions.length === 0 ? (
            <div className="no-actions">No actions available</div>
          ) : (
            availableActions.map((action) => (
              <div
                key={action.id}
                className={`action-item ${expandedAction === action.id ? 'expanded' : ''}`}
                onClick={() => handleActionClick(action.id)}
              >
                <div className="action-header">
                  <span className="action-name">{action.name}</span>
                  <span className="action-arrow">
                    {expandedAction === action.id ? '▼' : '▶'}
                  </span>
                </div>
                {expandedAction === action.id && (
                  <div className="action-details">
                    <div className="action-costs">
                      <span className="cost-label">Cost:</span>
                      {Object.entries(action.cost).map(([metric, value]) => (
                        <span key={metric} className="cost-item">
                          {formatMetricName(metric)}: {value > 0 ? '+' : ''}{value.toFixed(0)}
                        </span>
                      ))}
                    </div>
                    <div className="action-effects">
                      <span className="effect-label">Effects:</span>
                      {Object.entries(action.effects).map(([metric, value]) => (
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
                        onActionSubmit(action.id);
                      }}
                      disabled={isLoading || actionsLimitReached}
                    >
                      Execute
                    </button>
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
                className={`event-item ${event.is_key ? 'key-event' : ''}`}
              >
                <div className="event-year">{event.year}</div>
                <div className="event-description">{event.description}</div>
                {event.is_key && <span className="key-badge">KEY</span>}
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
