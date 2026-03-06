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
          <div className="narrative-loading">
            <span className="loading-spinner"></span>
            <span className="loading-text">Generating<span className="dots">...</span></span>
          </div>
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
