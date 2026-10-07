import React, { useState } from 'react';
import type { ChronicleEntry } from '../types';
import './ChroniclePanel.css';

interface ChroniclePanelProps {
  /// The game's book; the panel shows the chronicles of past half-years (before `currentTick`).
  entries: ChronicleEntry[];
  currentTick: number;
}

/// «Летопись»: past chronicles of the game, newest first. They are kept, not erased, when a new
/// chronicle is written; the chronicler never reads them.
export const ChroniclePanel: React.FC<ChroniclePanelProps> = ({ entries, currentTick }) => {
  const [open, setOpen] = useState(false);
  const past = entries.filter((e) => e.tick < currentTick).slice().reverse();
  return (
    <div className="chronicle-panel">
      <button type="button" className="chronicle-title" onClick={() => setOpen((v) => !v)} aria-expanded={open}>
        Летопись ({past.length}) {open ? '▾' : '▸'}
      </button>
      {open && (
        <div className="chronicle-list">
          {past.length === 0 ? (
            <div className="chronicle-empty">Прошлых хроник пока нет</div>
          ) : (
            past.map((e) => (
              <article key={e.tick} className="chronicle-entry">
                <h4 className="chronicle-date">{e.half_year} {e.year} года</h4>
                <p className="chronicle-text">{e.text}</p>
              </article>
            ))
          )}
        </div>
      )}
    </div>
  );
};

export default ChroniclePanel;
