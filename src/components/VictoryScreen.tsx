import type { WorldState } from '../types';
import './VictoryScreen.css';

interface VictoryScreenProps {
  worldState: WorldState;
  victoryTitle: string;
  victoryDescription: string;
  onContinue: () => void;
  onNewGame: () => void;
}

export default function VictoryScreen({
  worldState,
  victoryTitle,
  victoryDescription,
  onContinue,
  onNewGame,
}: VictoryScreenProps) {
  return (
    <div className="victory-overlay">
      <div className="victory-panel">
        <div className="victory-year">{worldState.year} год</div>
        <h1 className="victory-title">{victoryTitle}</h1>
        <p className="victory-description">{victoryDescription}</p>
        <div className="victory-stats">
          <span>Тиков: {worldState.tick}</span>
        </div>
        <div className="victory-actions">
          <button onClick={onContinue}>Продолжить наблюдение</button>
          <button onClick={onNewGame}>Новая игра</button>
        </div>
      </div>
    </div>
  );
}
