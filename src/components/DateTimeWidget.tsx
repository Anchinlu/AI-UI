import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import './DateTimeWidget.css';

interface SystemInfo {
  time: string;
  date: string;
  battery_percent: number;
  is_charging: boolean;
}

const SaturnBattery = ({ percent, isCharging }: { percent: number; isCharging: boolean }) => {
  const r = 9;
  const cy = 16;
  // Calculate water level: 0% = y:25, 100% = y:7
  const waterHeight = (percent / 100) * (r * 2);
  const waterY = cy + r - waterHeight;

  return (
    <div className={`battery-wrapper saturn-battery ${isCharging ? 'charging' : ''}`}>
      <svg width="32" height="32" viewBox="0 0 32 32" className="battery-svg">
        <defs>
          <clipPath id="water-clip">
            <rect x="0" y={waterY} width="32" height="32" className="water-level-rect" />
          </clipPath>
          <clipPath id="front-ring-clip">
            <rect x="0" y="16" width="32" height="16" />
          </clipPath>
        </defs>
        
        {/* Back half of the ring */}
        <g transform="rotate(-20 16 16)">
          <ellipse 
            cx="16" cy="16" rx="14" ry="4.5" 
            fill="none" stroke="rgba(255,255,255,0.85)" 
            strokeWidth="1.5" strokeLinecap="round"
            pathLength="100" strokeDasharray="70 30" 
            className="saturn-ring" 
          />
        </g>

        {/* Planet Outline & Background */}
        <circle cx="16" cy="16" r={r} stroke="rgba(255,255,255,0.95)" strokeWidth="1.5" fill="#000000" />
        
        {/* Liquid Water Fill */}
        <circle cx="16" cy="16" r={r} fill="rgba(255,255,255,0.95)" clipPath="url(#water-clip)" className="saturn-water" />
        
        {/* Charging Lightning Bolt Overlay */}
        {isCharging && (
          <path d="M16.5 10.5L12.5 16h3.5l-0.5 5 4-6h-3.5z" fill="#000000" stroke="rgba(255,255,255,0.9)" strokeWidth="0.5" className="charging-spark" />
        )}

        {/* Front half of the ring */}
        <g transform="rotate(-20 16 16)">
          <ellipse 
            cx="16" cy="16" rx="14" ry="4.5" 
            fill="none" stroke="rgba(255,255,255,0.85)" 
            strokeWidth="1.5" strokeLinecap="round"
            pathLength="100" strokeDasharray="70 30" 
            className="saturn-ring" clipPath="url(#front-ring-clip)" 
          />
        </g>
      </svg>
    </div>
  );
};

export function DateTimeWidget() {
  const [sysInfo, setSysInfo] = useState<SystemInfo>({
    time: '--:--',
    date: '---',
    battery_percent: 0,
    is_charging: false
  });

  useEffect(() => {
    let isMounted = true;
    
    const fetchInfo = async () => {
      try {
        const info: SystemInfo = await invoke('get_system_info');
        if (isMounted) {
          setSysInfo(info);
        }
      } catch (err) {
        console.warn('Failed to get system info (using fallback):', err);
        // Retain last known state on failure
      }
    };

    fetchInfo();
    const interval = setInterval(fetchInfo, 2000); // every 2s

    return () => {
      isMounted = false;
      clearInterval(interval);
    };
  }, []);

  return (
    <div className="system-status-widget">
      <div className="battery-indicator">
        <SaturnBattery percent={sysInfo.battery_percent} isCharging={sysInfo.is_charging} />
        <span className="battery-text">{sysInfo.battery_percent}%</span>
      </div>
      <div className="datetime-group">
        <span className="time-text">{sysInfo.time}</span>
        <span className="date-text">{sysInfo.date}</span>
      </div>
    </div>
  );
}
