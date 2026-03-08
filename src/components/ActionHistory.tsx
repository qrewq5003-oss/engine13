import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import './ActionHistory.css';

interface ActionHistoryEntry {
  tick: number;
  year: number;
  action_id: string;
  action_name: string;
  effects_summary: string[];
}

interface ActionHistoryProps {
  tick: number;
}

export function ActionHistory({ tick }: ActionHistoryProps) {
  const [history, setHistory] = useState<ActionHistoryEntry[]>([]);

  useEffect(() => {
    invoke<ActionHistoryEntry[]>('cmd_get_action_history', { limit: 5 })
      .then(setHistory)
      .catch(err => console.error('Failed to get action history:', err));
  }, [tick]);

  if (history.length === 0) return null;

  return (
    <div className="action-history">
      <h3 className="action-history-title">История действий</h3>
      <div className="action-history-list">
        {history.map((entry, i) => (
          <div key={i} className="action-entry">
            <div className="action-entry-header">
              <span className="action-year">{entry.year}</span>
              <span className="action-name">{entry.action_name}</span>
            </div>
            <div className="action-effects">
              {entry.effects_summary.map((effect, j) => (
                <span
                  key={j}
                  className={`action-effect ${effect.includes('+') ? 'positive' : 'negative'}`}
                >
                  {effect}
                </span>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

export default ActionHistory;
