import React from 'react';
import './NarrativePanel.css';

interface NarrativePanelProps {
  narrative: string | null;
  isLoading: boolean;
}

export const NarrativePanel: React.FC<NarrativePanelProps> = ({ narrative, isLoading }) => {
  return (
    <div className="narrative-panel">
      <h3 className="narrative-title">Narrative</h3>
      <div className="narrative-content">
        {isLoading ? (
          <div className="narrative-loading">Generating narrative...</div>
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
