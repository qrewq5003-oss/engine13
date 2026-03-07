import React from 'react';
import type { SaveSlotList } from '../types';
import './SaveSlotModal.css';

interface SaveSlotModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSave: (slot: string) => void;
  saves: SaveSlotList | null;
}

export const SaveSlotModal: React.FC<SaveSlotModalProps> = ({
  isOpen,
  onClose,
  onSave,
  saves,
}) => {
  if (!isOpen) return null;

  const handleSlotClick = (slot: string) => {
    onSave(slot);
    onClose();
  };

  return (
    <div className="save-slot-modal-overlay" onClick={onClose}>
      <div className="save-slot-modal" onClick={(e) => e.stopPropagation()}>
        <h2 className="save-slot-title">Сохранить игру</h2>
        
        <div className="save-slots-grid">
          {/* Auto save slot */}
          <button
            className="save-slot-card"
            onClick={() => handleSlotClick('auto')}
          >
            <div className="save-slot-header">
              <span className="save-slot-name">Автосохранение</span>
            </div>
            {saves?.auto ? (
              <div className="save-slot-info">
                <span className="save-slot-year">{saves.auto.year} AD</span>
                <span className="save-slot-tick">Тик {saves.auto.tick}</span>
              </div>
            ) : (
              <span className="save-slot-empty">Пусто</span>
            )}
          </button>

          {/* Manual save slots */}
          {saves?.slots.map((slot, index) => (
            <button
              key={`slot_${index + 1}`}
              className="save-slot-card"
              onClick={() => handleSlotClick(`slot_${index + 1}`)}
            >
              <div className="save-slot-header">
                <span className="save-slot-name">Слот {index + 1}</span>
              </div>
              {slot ? (
                <div className="save-slot-info">
                  <span className="save-slot-year">{slot.year} AD</span>
                  <span className="save-slot-tick">Тик {slot.tick}</span>
                </div>
              ) : (
                <span className="save-slot-empty">Пусто</span>
              )}
            </button>
          ))}
        </div>

        <button className="save-slot-close" onClick={onClose}>
          Отмена
        </button>
      </div>
    </div>
  );
};

export default SaveSlotModal;
