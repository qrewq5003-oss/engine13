import React from 'react';
import './NarrativePanel.css';

interface NarrativePanelProps {
  narrative: string | null;
  isLoading: boolean;
  error: string | null;
}

export const NarrativePanel: React.FC<NarrativePanelProps> = ({ narrative, isLoading, error }) => {
  return (
    <div className="narrative-panel">
      <h3 className="narrative-title">Narrative</h3>
      <div className="narrative-content">
        {isLoading ? (
          <div className="narrative-loading">Generating narrative...</div>
        ) : error ? (
          <div className="narrative-error">{error}</div>
        ) : narrative ? (
          <p className="narrative-text">{narrative}</p>
        ) : (
          <div className="narrative-placeholder">No narrative yet</div>
        )}
      </div>
    </div>
  );
};

export default NarrativePanel;
