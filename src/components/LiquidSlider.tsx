import React, { useRef, useState } from 'react';
import './LiquidSlider.css';

type LiquidSliderProps = {
  value: number;
  min?: number;
  max?: number;
  disabled?: boolean;
  onChange: (value: number) => void;
  ariaLabel: string;
};

export const LiquidSlider: React.FC<LiquidSliderProps> = ({
  value,
  min = 0,
  max = 100,
  disabled = false,
  onChange,
  ariaLabel
}) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const [isDragging, setIsDragging] = useState(false);
  const pointerIdRef = useRef<number | null>(null);

  // Normalize value between 0 and 1
  const percentage = Math.max(0, Math.min(1, (value - min) / (max - min)));

  const handlePointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (disabled) return;
    setIsDragging(true);
    pointerIdRef.current = e.pointerId;
    e.currentTarget.setPointerCapture(e.pointerId);
    updateValueFromEvent(e);
  };

  const handlePointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!isDragging || disabled || pointerIdRef.current === null) return;
    updateValueFromEvent(e);
  };

  const handlePointerUp = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!isDragging || disabled) return;
    setIsDragging(false);
    if (pointerIdRef.current !== null) {
      try {
        e.currentTarget.releasePointerCapture(pointerIdRef.current);
      } catch {
        // ignore
      }
      pointerIdRef.current = null;
    }
  };

  const updateValueFromEvent = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!containerRef.current) return;
    const rect = containerRef.current.getBoundingClientRect();
    let newPercent = (e.clientX - rect.left) / rect.width;
    newPercent = Math.max(0, Math.min(1, newPercent));
    const newValue = Math.round(min + newPercent * (max - min));
    onChange(newValue);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (disabled) return;
    const step = (max - min) / 20; // 5% step
    let newValue = value;
    if (e.key === 'ArrowRight' || e.key === 'ArrowUp') {
      newValue = Math.min(max, value + step);
    } else if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') {
      newValue = Math.max(min, value - step);
    } else if (e.key === 'Home') {
      newValue = min;
    } else if (e.key === 'End') {
      newValue = max;
    } else {
      return;
    }
    e.preventDefault();
    onChange(Math.round(newValue));
  };

  return (
    <div
      ref={containerRef}
      className={`liquid-slider-container ${disabled ? 'disabled' : ''}`}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      onPointerCancel={handlePointerUp}
      onKeyDown={handleKeyDown}
      tabIndex={disabled ? -1 : 0}
      role="slider"
      aria-valuemin={min}
      aria-valuemax={max}
      aria-valuenow={value}
      aria-label={ariaLabel}
    >
      <svg width="100%" height="100%" preserveAspectRatio="none" className="liquid-slider-svg">
        <defs>
          <clipPath id="liquid-slider-clip">
            <rect width="100%" height="100%" rx="4" />
          </clipPath>
        </defs>
        
        {/* Background track */}
        <rect width="100%" height="100%" rx="4" className="liquid-slider-track" />
        
        {/* Fill (Water) */}
        <rect 
          width={`${percentage * 100}%`} 
          height="100%" 
          clipPath="url(#liquid-slider-clip)"
          className={`liquid-slider-fill ${isDragging ? 'dragging' : ''}`} 
        />
        
        {/* Glow/Reflection effect */}
        <rect 
          width={`${percentage * 100}%`} 
          height="40%" 
          clipPath="url(#liquid-slider-clip)"
          className="liquid-slider-reflection"
          style={{ transition: isDragging ? 'none' : 'width 0.2s ease-out' }}
        />
      </svg>
    </div>
  );
};
