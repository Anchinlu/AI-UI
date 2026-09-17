import { useRef, useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useAutoHide } from '../hooks/useAutoHide';
import { DateTimeWidget } from './DateTimeWidget';
import { LiquidSlider } from './LiquidSlider';
import { AiOrbPanel, type AiState } from './AiOrbPanel';
import { 
  Volume2, 
  Monitor,
  Wifi, 
  LayoutGrid, 
  Zap, 
  Settings,
  ChevronLeft
} from 'lucide-react';
import { listen } from '@tauri-apps/api/event';
import '../animations/liquid-glass.css';

const SATELLITES = [
  { id: 'audio', label: 'Audio', icon: Volume2, color: '#4285F4' }, // Blue
  { id: 'display', label: 'Display', icon: Monitor, color: '#EA4335' }, // Red
  { id: 'connectivity', label: 'Connectivity', icon: Wifi, color: '#FBBC05' }, // Yellow
  { id: 'apps', label: 'Apps', icon: LayoutGrid, color: '#34A853' }, // Green
  { id: 'quick_actions', label: 'Actions', icon: Zap, color: '#4285F4' }, // Blue
  { id: 'system', label: 'System', icon: Settings, color: '#EA4335' }, // Red
];

export function TaskbarShell() {
  const barRef = useRef<HTMLDivElement>(null);
  const { setIsLocked, stopAnimation } = useAutoHide(barRef);

  const [dragY, setDragY] = useState(0);
  const startYRef = useRef(0);
  const pointerActiveRef = useRef(false);
  const pointerIdRef = useRef<number | null>(null);
  type WindowMode = 'hidden' | 'bar' | 'ai-input' | 'radial' | 'dragging';
  const [currentMode, setCurrentMode] = useState<WindowMode>('bar');
  
  const isRadialOpen = currentMode === 'radial';
  const isInputOpen = currentMode === 'ai-input';
  
  // === AI Prompt State ===
  const [aiState, setAiState] = useState<AiState>('idle');
  const [prompt, setPrompt] = useState('');
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const activeRequestId = useRef<string | null>(null);

  // === Conversation History (H2) ===
  type ChatRole = 'user' | 'assistant';
  type ConversationMessage = { id: string; role: ChatRole; content: string };
  const [committedMessages, setCommittedMessages] = useState<ConversationMessage[]>([]);
  const pendingTurnRef = useRef<{ requestId: string; userContent: string; assistantContent: string } | null>(null);
  const [streamingText, setStreamingText] = useState<string>('');

  useEffect(() => {
    if (currentMode !== 'ai-input' && activeRequestId.current) {
      invoke('ai_stop', { requestId: activeRequestId.current }).catch(console.error);
      activeRequestId.current = null;
      pendingTurnRef.current = null;
      setStreamingText('');
      setAiState('idle');
    }
  }, [currentMode]);

  const handleOrbClick = async () => {
    if (currentMode === 'radial' || currentMode === 'dragging') return;

    if (currentMode !== 'ai-input') {
      stopAnimation();
      setIsLocked(true);
      
      // Reset transient AI state, keep conversation history
      setAiState('idle');
      setErrorMsg(null);
      setStreamingText('');
      
      await invoke('expand_window', { height: 400 }).catch(console.error);
      invoke('set_interaction_mode', { mode: 'radial' }).catch(console.error);
      setCurrentMode('ai-input');
    } else {
      // Đóng AI Prompt
      setCurrentMode('bar');
      await invoke('shrink_window').catch(console.error);
      invoke('set_interaction_mode', { mode: 'bar' }).catch(console.error);
      setIsLocked(false);
    }
  };

  const clearConversation = () => {
    if (activeRequestId.current) {
      invoke('ai_stop', { requestId: activeRequestId.current }).catch(console.error);
      activeRequestId.current = null;
    }
    pendingTurnRef.current = null;
    setCommittedMessages([]);
    setStreamingText('');
    setErrorMsg(null);
    setAiState('idle');
  };

  const handleAiKeyDown = async (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter' && prompt.trim() !== '') {
      e.preventDefault();
      
      // Stop existing request and discard pending turn
      if (activeRequestId.current) {
        invoke('ai_stop', { requestId: activeRequestId.current }).catch(console.error);
        pendingTurnRef.current = null;
        setStreamingText('');
      }

      const reqId = crypto.randomUUID();
      activeRequestId.current = reqId;
      const userContent = prompt.trim();

      // Build messages: committed history + new user message
      const requestMessages = [
        ...committedMessages.map(m => ({ role: m.role, content: m.content })),
        { role: 'user', content: userContent }
      ];

      // Set pending turn
      pendingTurnRef.current = { requestId: reqId, userContent, assistantContent: '' };

      setAiState('thinking');
      setErrorMsg(null);
      setStreamingText('');
      setPrompt('');

      try {
        await invoke('ai_stream', { 
          messages: requestMessages, 
          params: { request_id: reqId, stream: true } 
        });
      } catch (err) {
        if (activeRequestId.current === reqId) {
          setErrorMsg(String(err));
          setAiState('error');
          pendingTurnRef.current = null;
          setStreamingText('');
        }
      }
    }
  };

  useEffect(() => {
    const unlistenChunkPromise = listen('ai-stream-chunk', (event: any) => {
      const { text, done, request_id } = event.payload;
      if (request_id !== activeRequestId.current) return;

      setAiState('speaking');
      
      if (pendingTurnRef.current && pendingTurnRef.current.requestId === request_id) {
        pendingTurnRef.current.assistantContent += text;
        setStreamingText(pendingTurnRef.current.assistantContent);
      }
      
      if (done) {
        // Commit the turn to history
        if (pendingTurnRef.current && pendingTurnRef.current.requestId === request_id) {
          const pt = pendingTurnRef.current;
          setCommittedMessages(prev => [
            ...prev,
            { id: crypto.randomUUID(), role: 'user' as ChatRole, content: pt.userContent },
            { id: crypto.randomUUID(), role: 'assistant' as ChatRole, content: pt.assistantContent },
          ]);
          pendingTurnRef.current = null;
          setStreamingText('');
        }
        activeRequestId.current = null;
        setTimeout(() => setAiState('idle'), 500);
      }
    });

    const unlistenErrorPromise = listen('ai-stream-error', (event: any) => {
      const { error, request_id } = event.payload;
      if (request_id !== activeRequestId.current) return;

      setErrorMsg(error);
      setAiState('error');
      activeRequestId.current = null;
      // Discard pending turn on error
      pendingTurnRef.current = null;
      setStreamingText('');
    });

    return () => {
      unlistenChunkPromise.then(f => f());
      unlistenErrorPromise.then(f => f());
    };
  }, []);

  const [hoveredNode, setHoveredNode] = useState<number | null>(null);
  const [selectedNode, setSelectedNode] = useState<number | null>(null);
  const [orbitSpeed] = useState<number>(20);
  const [isOrbitPaused] = useState<boolean>(false);

  // 3D Orbit rAF Refs
  const orbitAngleRef = useRef(0);
  const lastTimeRef = useRef(0);
  const orbitalsContainerRef = useRef<HTMLDivElement>(null);
  const nodesRef = useRef<(HTMLDivElement | null)[]>([]);

  // === Audio state (component-level, not inside render fn) ===
  interface AudioStatus {
    volume: number;
    muted: boolean;
    output_device: string | null;
  }
  const [audioStatus, setAudioStatus] = useState<AudioStatus | null>(null);
  const [audioError, setAudioError] = useState<string | null>(null);
  const volumeDebounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Fetch audio status when Audio panel is opened
  useEffect(() => {
    if (selectedNode !== 0) {
      // Cleanup debounce when leaving Audio panel
      if (volumeDebounceRef.current) {
        clearTimeout(volumeDebounceRef.current);
        volumeDebounceRef.current = null;
      }
      return;
    }

    const fetchAudio = async () => {
      try {
        setAudioError(null);
        const status = await invoke<AudioStatus>('get_audio_status');
        setAudioStatus(status);
      } catch (err) {
        console.error('get_audio_status failed:', err);
        setAudioError(String(err));
      }
    };
    fetchAudio();

    return () => {
      if (volumeDebounceRef.current) {
        clearTimeout(volumeDebounceRef.current);
        volumeDebounceRef.current = null;
      }
    };
  }, [selectedNode]);

  const handleVolumeChange = (newVolume: number) => {
    const prev = audioStatus;
    setAudioStatus(s => s ? { ...s, volume: newVolume } : s);

    if (volumeDebounceRef.current) {
      clearTimeout(volumeDebounceRef.current);
    }
    volumeDebounceRef.current = setTimeout(() => {
      invoke('set_volume', { level: newVolume }).catch((err) => {
        console.error('set_volume failed:', err);
        // Rollback
        if (prev) setAudioStatus(prev);
      });
    }, 100);
  };

  const handleMuteToggle = () => {
    const prev = audioStatus;
    const newMuted = !audioStatus?.muted;
    setAudioStatus(s => s ? { ...s, muted: newMuted } : s);

    invoke('set_mute', { muted: newMuted }).catch((err) => {
      console.error('set_mute failed:', err);
      // Rollback
      if (prev) setAudioStatus(prev);
    });
  };

  // === Display state (component-level) ===
  interface DisplayInfo {
    id: string;
    name: string;
    is_primary: boolean;
    brightness: number | null;
    supports_brightness: boolean;
  }
  interface DisplayStatus {
    monitors: DisplayInfo[];
  }
  const [displayStatus, setDisplayStatus] = useState<DisplayStatus | null>(null);
  const [displayError, setDisplayError] = useState<string | null>(null);
  const [selectedMonitorId, setSelectedMonitorId] = useState<string | null>(null);
  const brightnessDebounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Fetch display status when Display panel is opened
  useEffect(() => {
    if (selectedNode !== 1) {
      if (brightnessDebounceRef.current) {
        clearTimeout(brightnessDebounceRef.current);
        brightnessDebounceRef.current = null;
      }
      return;
    }

    const fetchDisplay = async () => {
      try {
        setDisplayError(null);
        const status = await invoke<DisplayStatus>('get_display_status');
        setDisplayStatus(status);
        if (status.monitors.length > 0) {
          const primary = status.monitors.find(m => m.is_primary);
          setSelectedMonitorId(primary ? primary.id : status.monitors[0].id);
        }
      } catch (err) {
        console.error('get_display_status failed:', err);
        setDisplayError(String(err));
      }
    };
    fetchDisplay();

    return () => {
      if (brightnessDebounceRef.current) {
        clearTimeout(brightnessDebounceRef.current);
        brightnessDebounceRef.current = null;
      }
    };
  }, [selectedNode]);

  const handleBrightnessChange = (monitorId: string, newBrightness: number) => {
    const prev = displayStatus;
    
    // Optimistic update
    setDisplayStatus(s => {
      if (!s) return s;
      return {
        monitors: s.monitors.map(m => 
          m.id === monitorId ? { ...m, brightness: newBrightness } : m
        )
      };
    });

    if (brightnessDebounceRef.current) {
      clearTimeout(brightnessDebounceRef.current);
    }
    
    brightnessDebounceRef.current = setTimeout(() => {
      invoke('set_brightness', { monitorId, level: newBrightness }).catch((err) => {
        console.error('set_brightness failed:', err);
        // Rollback
        if (prev) setDisplayStatus(prev);
        setDisplayError(String(err));
      });
    }, 100);
  };

  // === Connectivity state (component-level) ===
  interface ConnectedDevice {
    name: string;
    device_type: string;
    connection_state: string;
  }
  interface ConnectivityStatus {
    wifi_enabled: boolean;
    bluetooth_enabled: boolean;
    detected_devices: ConnectedDevice[];
  }
  const [connectivityStatus, setConnectivityStatus] = useState<ConnectivityStatus | null>(null);
  const [connectivityLoading, setConnectivityLoading] = useState<boolean>(false);
  const [connectivityError, setConnectivityError] = useState<string | null>(null);

  // Fetch connectivity status when Connectivity panel is opened
  useEffect(() => {
    if (selectedNode !== 2) return;

    const fetchConnectivity = async () => {
      try {
        setConnectivityError(null);
        setConnectivityLoading(true);
        const status = await invoke<ConnectivityStatus>('get_connectivity_status');
        setConnectivityStatus(status);
        setConnectivityLoading(false);
      } catch (err) {
        console.error('get_connectivity_status failed:', err);
        setConnectivityError(String(err));
        setConnectivityLoading(false);
      }
    };
    fetchConnectivity();
  }, [selectedNode]);

  useEffect(() => {
    let reqId: number;

    const loop = (time: number) => {
      const dt = (time - lastTimeRef.current) / 1000;
      lastTimeRef.current = time;

      if (!isOrbitPaused && hoveredNode === null && selectedNode === null && isRadialOpen) {
        // Update angle (360 degrees per orbitSpeed seconds)
        orbitAngleRef.current += (360 / orbitSpeed) * dt;
      }

      // Always apply transforms to smoothly render current state
      if (orbitalsContainerRef.current) {
        orbitalsContainerRef.current.style.transform = `rotateX(70deg) rotateZ(${orbitAngleRef.current}deg)`;
      }

      nodesRef.current.forEach((node, i) => {
        if (!node) return;
        
        const nodeGlobalAngle = orbitAngleRef.current + i * 60;
        
        // Determine depth (cos(angle) < -0.15 means it's in the front half)
        const cosAngle = Math.cos((nodeGlobalAngle) * Math.PI / 180);
        const inFront = cosAngle < -0.15; // Slightly biased towards front for a wider clickable area

        // Apply scale based on depth for a 3D effect
        const scale = inFront ? 1.1 : 0.85;

        // Counter-rotate the node to stay upright, and apply scale
        node.style.transform = `rotateZ(-${orbitAngleRef.current}deg) rotateX(-70deg) scale(${scale})`;
        
        if (inFront) {
          node.classList.add('in-front');
          node.classList.remove('in-back');
        } else {
          node.classList.add('in-back');
          node.classList.remove('in-front');
        }
      });

      reqId = requestAnimationFrame(loop);
    };

    if (isRadialOpen) {
      lastTimeRef.current = performance.now();
      reqId = requestAnimationFrame(loop);
    }

    return () => cancelAnimationFrame(reqId);
  }, [isRadialOpen, isOrbitPaused, orbitSpeed, hoveredNode, selectedNode]);

  // === Phase 6.1: Send interactive zones to Rust (throttled ~100ms) ===
  const zonesThrottleRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const placeholderPanelRef = useRef<HTMLDivElement>(null);

  // === Phase 6.1: Escape key to close Radial ===
  useEffect(() => {
    const handleKeyDown = async (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        if (currentMode === 'radial') {
          setCurrentMode('bar');
          setIsLocked(false);
          setSelectedNode(null);
          if (barRef.current) {
            barRef.current.style.transform = `translateY(0px)`;
            barRef.current.style.width = `440px`;
            barRef.current.style.height = `80px`;
            barRef.current.style.borderRadius = `0 0 28px 28px`;
            barRef.current.style.background = '#000000';
            barRef.current.style.border = '1px solid rgba(255, 255, 255, 0.08)';
            barRef.current.style.boxShadow = '0 12px 32px rgba(0, 0, 0, 0.5), 0 2px 8px rgba(0, 0, 0, 0.3)';
            
            const content = barRef.current.querySelector('.taskbar-content') as HTMLElement;
            if (content) {
              const orb = content.querySelector('.ai-orb-panel') as HTMLElement;
              if (orb) orb.style.left = `24px`;
              
              const dateTime = content.querySelector('.datetime-widget-container') as HTMLElement;
              if (dateTime) {
                dateTime.style.opacity = `1`;
                dateTime.style.transform = `scale(1)`;
                dateTime.style.pointerEvents = 'auto';
              }
            }
          }
          await invoke('shrink_window').catch(console.error);
          invoke('set_interaction_mode', { mode: 'bar' }).catch(console.error);
        } else if (currentMode === 'ai-input') {
          setCurrentMode('bar');
          await invoke('shrink_window').catch(console.error);
          invoke('set_interaction_mode', { mode: 'bar' }).catch(console.error);
          setIsLocked(false);
        }
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [currentMode, setIsLocked]);

  // === Phase 7: Remove auto-resize based on dependencies ===
  // Window resizing is now handled sequentially during explicit actions like handleOrbClick or drag release.

  useEffect(() => {
    if (!isRadialOpen && !isInputOpen) {
      // Clear any pending throttle
      if (zonesThrottleRef.current) {
        clearTimeout(zonesThrottleRef.current);
        zonesThrottleRef.current = null;
      }
      return;
    }

    const sendZones = () => {
      const zones: { x: number; y: number; w: number; h: number }[] = [];

      // 1. AI Orb
      const orbEl = document.querySelector('.ai-orb-panel');
      if (orbEl) {
        const r = orbEl.getBoundingClientRect();
        zones.push({ x: r.left, y: r.top, w: r.width, h: r.height });
      }

      // 2. Orbital nodes that are in front (clickable)
      document.querySelectorAll('.orbital-node.in-front').forEach(node => {
        const r = node.getBoundingClientRect();
        zones.push({ x: r.left, y: r.top, w: r.width, h: r.height });
      });

      // 3. Placeholder panel (covers all controls inside)
      if (placeholderPanelRef.current) {
        const r = placeholderPanelRef.current.getBoundingClientRect();
        zones.push({ x: r.left, y: r.top, w: r.width, h: r.height });
      }

      // 4. Back button in panel header
      const backBtn = document.querySelector('.placeholder-header .back-btn');
      if (backBtn) {
        const r = backBtn.getBoundingClientRect();
        zones.push({ x: r.left, y: r.top, w: r.width, h: r.height });
      }

      // 5. AI Prompt Popup
      const promptPopup = document.querySelector('.ai-prompt-popup');
      if (promptPopup) {
        const r = promptPopup.getBoundingClientRect();
        zones.push({ x: r.left, y: r.top, w: r.width, h: r.height });
      }

      invoke('update_interactive_zones', { zones }).catch(console.error);
    };

    // Send zones immediately, then throttle at ~100ms
    sendZones();
    const interval = setInterval(sendZones, 100);

    return () => {
      clearInterval(interval);
      if (zonesThrottleRef.current) {
        clearTimeout(zonesThrottleRef.current);
        zonesThrottleRef.current = null;
      }
    };
  }, [isRadialOpen, isInputOpen, selectedNode]);

  const isInteractiveTarget = (target: EventTarget | null) =>
    target instanceof Element &&
    Boolean(target.closest('.orbital-node, .placeholder-panel, .ai-prompt-popup'));

  const isOrbTarget = (target: EventTarget | null) => 
    target instanceof Element &&
    Boolean(target.closest('.ai-orb-panel'));

  const handlePointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (isInteractiveTarget(e.target)) return;
    if (currentMode === 'radial' && !isOrbTarget(e.target)) return;

    pointerActiveRef.current = true;
    pointerIdRef.current = e.pointerId;
    // When in radial mode, the visual state corresponds to dragY = 120.
    // Setting this correctly ensures smooth dragging back up to 0.
    startYRef.current = e.clientY - (currentMode === 'radial' ? 120 : dragY);
  };

  const handlePointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!pointerActiveRef.current) return;
    
    const dyRaw = e.clientY - startYRef.current;
    
    if (currentMode !== 'dragging') {
      if (currentMode !== 'radial' && dyRaw > 5) {
        stopAnimation();
        setCurrentMode('dragging');
        setIsLocked(true);
        invoke('set_interaction_mode', { mode: 'dragging' }).catch(console.error);
        
        if (pointerIdRef.current !== null) {
          e.currentTarget.setPointerCapture(pointerIdRef.current);
        }
        invoke('expand_window', { height: 600 }).catch(console.error);
      } else if (currentMode === 'radial' && dyRaw < -5) {
        stopAnimation();
        setCurrentMode('dragging');
        setIsLocked(true);
        invoke('set_interaction_mode', { mode: 'dragging' }).catch(console.error);
        
        if (pointerIdRef.current !== null) {
          e.currentTarget.setPointerCapture(pointerIdRef.current);
        }
      }
    }
    
    if (currentMode !== 'dragging') return;
    
    if (dyRaw >= 0) { // dragging down
      const dy = Math.max(0, dyRaw);
      setDragY(dy);
      
      if (barRef.current) {
        barRef.current.style.transition = 'none';
        const squashW = Math.max(160, 440 - dy * 0.8);
        const stretchH = 80 + dy;
        const rawRadius = 28 + dy * 1.5;
        const radius = Math.min(rawRadius, squashW / 2);
        
        barRef.current.style.transform = `translateY(0px)`; 
        barRef.current.style.width = `${squashW}px`;
        barRef.current.style.height = `${stretchH}px`;
        
        const svgPaths = barRef.current.querySelectorAll('.silhouette-path');
        svgPaths.forEach(svgPath => {
          (svgPath as HTMLElement).style.transition = 'none';
          const w = squashW;
          const h = stretchH;
          const r = radius;
          svgPath.setAttribute('d', `M 0,0 A 32,32 0 0,1 32,32 L 32,${h - r} A ${r},${r} 0 0,0 ${32 + r},${h} L ${32 + w - r},${h} A ${r},${r} 0 0,0 ${32 + w},${h - r} L ${32 + w},32 A 32,32 0 0,1 ${64 + w},0 Z`);
        });
        
        const content = barRef.current.querySelector('.taskbar-content') as HTMLElement;
        if (content) {
          content.style.transition = 'none';
          const progress = Math.min(1, dy / 80);
          
          const orb = content.querySelector('.ai-orb-panel') as HTMLElement;
          if (orb) {
            orb.style.transition = 'none';
            const targetLeft = (squashW - 80) / 2;
            const currentLeft = 24 + (targetLeft - 24) * progress;
            orb.style.left = `${currentLeft}px`;
          }
          
          const dateTime = content.querySelector('.datetime-widget-container') as HTMLElement;
          if (dateTime) {
            dateTime.style.transition = 'none';
            dateTime.style.opacity = `${1 - progress * 1.5}`;
            dateTime.style.transform = `scale(${1 - progress * 0.5})`;
            dateTime.style.pointerEvents = progress > 0.1 ? 'none' : 'auto';
          }
        }
      }
    } else { // dragging up
      const dy = Math.max(-60, dyRaw); // Prevent dragging too far up
      setDragY(dy);
      
      if (barRef.current) {
        barRef.current.style.transition = 'none';
        barRef.current.style.transform = `translateY(0px)`; // anchor at top
        
        // Pushing against ceiling animation (dy is negative)
        const flattenW = 440 + Math.abs(dy) * 1.5;
        const rawRadius = Math.max(0, 28 - Math.abs(dy) * 0.5);
        const radius = Math.min(rawRadius, flattenW / 2);
        const flattenH = Math.max(32 + radius, 80 - Math.abs(dy));
        
        barRef.current.style.width = `${flattenW}px`;
        barRef.current.style.height = `${flattenH}px`;
        
        const svgPaths = barRef.current.querySelectorAll('.silhouette-path');
        svgPaths.forEach(svgPath => {
          (svgPath as HTMLElement).style.transition = 'none';
          const w = flattenW;
          const h = flattenH;
          const r = radius;
          svgPath.setAttribute('d', `M 0,0 A 32,32 0 0,1 32,32 L 32,${h - r} A ${r},${r} 0 0,0 ${32 + r},${h} L ${32 + w - r},${h} A ${r},${r} 0 0,0 ${32 + w},${h - r} L ${32 + w},32 A 32,32 0 0,1 ${64 + w},0 Z`);
        });
        
        const content = barRef.current.querySelector('.taskbar-content') as HTMLElement;
        if (content) {
          content.style.transition = 'none';
          const progress = Math.min(1, Math.abs(dy) / 60);
          content.style.opacity = `${1 - progress}`;
          content.style.transform = `scale(${1 - progress * 0.2})`;
        }
      }
    }
  };

  const handlePointerUp = async (e: React.PointerEvent<HTMLDivElement>) => {
    if (currentMode === 'dragging' && pointerIdRef.current !== null) {
      try {
        e.currentTarget.releasePointerCapture(pointerIdRef.current);
      } catch (err) {
        console.warn("Failed to release pointer capture", err);
      }
    }
    
    const wasTracking = pointerActiveRef.current;
    const wasDragging = currentMode === 'dragging';
    
    pointerActiveRef.current = false;
    pointerIdRef.current = null;
    
    if (!wasTracking) return;
    
    if (barRef.current) {
      barRef.current.style.transition = 'transform 0.35s ease, opacity 0.25s ease, width 0.35s ease, height 0.35s ease, border-radius 0.35s ease';
      const svgPaths = barRef.current.querySelectorAll('.silhouette-path');
      svgPaths.forEach(p => (p as HTMLElement).style.transition = 'd 0.35s ease');
    }
    
    let shouldOpen = false;
    let shouldClose = false;

    if (dragY >= 0) { // was dragged down
      shouldClose = !wasDragging;
      shouldOpen = wasDragging && dragY >= 80;
    } else { // was dragged up
      shouldClose = wasDragging && dragY <= -50;
      shouldOpen = wasDragging && dragY > -50; 
    }

    if (shouldClose || (dragY >= 0 && !shouldOpen && wasDragging)) {
      setDragY(0);
      setCurrentMode('bar');
      setSelectedNode(null);
      
      if (barRef.current) {
        barRef.current.style.transform = `translateY(0px)`;
        barRef.current.style.width = `440px`;
        barRef.current.style.height = `80px`;
        barRef.current.style.borderRadius = `0`;
        barRef.current.style.background = 'transparent';
        barRef.current.style.border = 'none';
        barRef.current.style.boxShadow = 'none';
        
        const svgPaths = barRef.current.querySelectorAll('.silhouette-path');
        svgPaths.forEach(svgPath => {
          svgPath.setAttribute('d', `M 0,0 A 32,32 0 0,1 32,32 L 32,52 A 28,28 0 0,0 60,80 L 444,80 A 28,28 0 0,0 472,52 L 472,32 A 32,32 0 0,1 504,0 Z`);
        });
        
        const content = barRef.current.querySelector('.taskbar-content') as HTMLElement;
        if (content) {
          content.style.transition = 'transform 0.35s ease, opacity 0.25s ease';
          content.style.opacity = '1';
          content.style.transform = 'scale(1)';
          
          const orb = content.querySelector('.ai-orb-panel') as HTMLElement;
          if (orb) {
            orb.style.transition = 'left 0.35s ease';
            orb.style.left = `24px`;
          }
          
          const dateTime = content.querySelector('.datetime-widget-container') as HTMLElement;
          if (dateTime) {
            dateTime.style.transition = 'opacity 0.25s ease, transform 0.35s ease';
            dateTime.style.opacity = `1`;
            dateTime.style.transform = `scale(1)`;
            dateTime.style.pointerEvents = 'auto';
          }
        }
        
        setTimeout(() => {
          if (barRef.current) barRef.current.style.transition = 'none';
        }, 500);
      }
      await invoke('shrink_window').catch(console.error);
      invoke('set_interaction_mode', { mode: 'bar' }).catch(console.error);
      setIsLocked(false);
    } else if (shouldOpen) {
      setDragY(120);
      setCurrentMode('radial');
      
      if (barRef.current) {
        barRef.current.style.transform = `translateY(0px)`;
        barRef.current.style.width = `340px`;
        barRef.current.style.height = `340px`;
        barRef.current.style.background = 'transparent';
        barRef.current.style.border = 'none';
        barRef.current.style.boxShadow = 'none';
        barRef.current.style.backdropFilter = 'none';
        
        const content = barRef.current.querySelector('.taskbar-content') as HTMLElement;
        if (content) {
          const orb = content.querySelector('.ai-orb-panel') as HTMLElement;
          if (orb) {
            orb.style.left = `130px`;
          }
        }
        
        setTimeout(() => {
          if (barRef.current) barRef.current.style.transition = 'none';
        }, 500);
      }
      invoke('set_interaction_mode', { mode: 'radial' }).catch(console.error);
    }
  };

  const renderPlaceholderContent = (index: number) => {
    switch (index) {
      case 0: // Audio
        return (
          <div className="panel-controls">
            {audioError && (
              <div className="control-row" style={{ color: '#EA4335', fontSize: '11px' }}>⚠ {audioError}</div>
            )}
            <div className="control-row">
              <label>Volume</label>
              <div style={{ flex: 1, padding: '0 8px' }}>
                <LiquidSlider 
                  value={audioStatus?.volume ?? 50}
                  onChange={handleVolumeChange}
                  ariaLabel="Volume"
                />
              </div>
              <span style={{ minWidth: '32px', textAlign: 'right', fontSize: '12px' }}>{audioStatus?.volume ?? '—'}%</span>
            </div>
            <div className="control-row">
              <label>Output</label>
              <span style={{ fontSize: '12px', opacity: 0.7 }}>{audioStatus?.output_device ?? 'Loading...'}</span>
            </div>
            <div className="control-row">
              <label>Mute</label>
              <button
                className={`btn-toggle ${audioStatus?.muted ? 'active' : ''}`}
                onClick={handleMuteToggle}
              >
                {audioStatus?.muted ? 'Muted' : 'Unmuted'}
              </button>
            </div>
          </div>
        );
      case 1: { // Display
        const activeMonitor = displayStatus?.monitors.find(m => m.id === selectedMonitorId);
        
        return (
          <div className="panel-controls">
            {displayError && (
              <div className="control-row" style={{ color: '#EA4335', fontSize: '11px', lineHeight: 1.2 }}>
                ⚠ {displayError}
              </div>
            )}
            
            <div className="control-row">
              <label>Monitor</label>
              <select 
                value={selectedMonitorId ?? ''} 
                onChange={(e) => setSelectedMonitorId(e.target.value)}
                style={{ flex: 2, maxWidth: '140px', textOverflow: 'ellipsis' }}
              >
                {displayStatus?.monitors.map(m => (
                  <option key={m.id} value={m.id}>
                    {m.name} {m.is_primary ? '(Primary)' : ''}
                  </option>
                ))}
              </select>
            </div>
            
            {activeMonitor && (
              <div className="control-row">
                <label>Brightness</label>
                {activeMonitor.supports_brightness ? (
                  <>
                    <div style={{ flex: 1, padding: '0 8px' }}>
                      <LiquidSlider 
                        value={activeMonitor.brightness ?? 0}
                        onChange={(newVal) => handleBrightnessChange(activeMonitor.id, newVal)}
                        ariaLabel="Brightness"
                      />
                    </div>
                    <span style={{ minWidth: '32px', textAlign: 'right', fontSize: '12px' }}>
                      {activeMonitor.brightness ?? '—'}%
                    </span>
                  </>
                ) : (
                  <span style={{ fontSize: '12px', opacity: 0.6, flex: 1, textAlign: 'right' }}>
                    Unsupported Display
                  </span>
                )}
              </div>
            )}
          </div>
        );
      }
      case 2: // Connectivity
        return (
          <div className="panel-controls">
            {connectivityLoading && (
              <div className="control-row" style={{ justifyContent: 'center', opacity: 0.7, fontSize: '12px' }}>
                Đang quét thiết bị...
              </div>
            )}
            
            {connectivityError && !connectivityLoading && (
              <div className="control-row" style={{ color: '#EA4335', fontSize: '11px', lineHeight: 1.2 }}>
                ⚠ {connectivityError}
              </div>
            )}
            
            {!connectivityLoading && connectivityStatus && (
              <>
                <div className="control-row">
                  <label>Wi-Fi <span style={{fontSize:'9px', opacity:0.5, fontWeight:'normal'}}>(Read-only)</span></label>
                  <button className={`btn-toggle ${connectivityStatus.wifi_enabled ? 'active' : ''}`} style={{ pointerEvents: 'none' }}>
                    {connectivityStatus.wifi_enabled ? 'On' : 'Off'}
                  </button>
                </div>
                
                <div className="control-row">
                  <label>Bluetooth <span style={{fontSize:'9px', opacity:0.5, fontWeight:'normal'}}>(Read-only)</span></label>
                  <button className={`btn-toggle ${connectivityStatus.bluetooth_enabled ? 'active' : ''}`} style={{ pointerEvents: 'none' }}>
                    {connectivityStatus.bluetooth_enabled ? 'On' : 'Off'}
                  </button>
                </div>

                {connectivityStatus.detected_devices.length > 0 && (
                  <div style={{ marginTop: '12px', borderTop: '1px solid rgba(255,255,255,0.1)', paddingTop: '12px' }}>
                    <div style={{ fontSize: '11px', opacity: 0.5, marginBottom: '8px', textTransform: 'uppercase', letterSpacing: '0.5px' }}>
                      Thiết bị ngoại vi (Detected)
                    </div>
                    {connectivityStatus.detected_devices.map((device, idx) => (
                      <div key={idx} className="control-row" style={{ marginBottom: '6px' }}>
                        <span style={{ fontSize: '12px', flex: 1, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
                          {device.device_type === 'wifi' ? '🌐 ' : '🛜 '}
                          {device.name}
                        </span>
                        <span style={{ fontSize: '10px', opacity: 0.7, textTransform: 'capitalize' }}>
                          {device.connection_state}
                        </span>
                      </div>
                    ))}
                  </div>
                )}
              </>
            )}
          </div>
        );
      case 3: // Apps
        return (
          <div className="panel-grid">
            <button className="grid-item">Browser</button>
            <button className="grid-item">Files</button>
            <button className="grid-item">Notes</button>
            <button className="grid-item">Settings</button>
          </div>
        );
      case 4: // Quick Actions
        return (
          <div className="panel-grid">
            <button className="grid-item">Screenshot</button>
            <button className="grid-item">Do Not Disturb</button>
            <button className="grid-item">Lock Screen</button>
            <button className="grid-item">Clipboard</button>
          </div>
        );
      case 5: // System
        return (
          <div className="panel-controls">
            <div className="control-row"><span>Battery</span><span>85% (Charging)</span></div>
            <div className="control-row"><span>Power Mode</span><span>Balanced</span></div>
            <button className="btn-full">Restart App</button>
          </div>
        );
      default:
        return null;
    }
  };

  return (
    <div className="taskbar-shell">
      <div 
        ref={barRef} 
        className={`taskbar-bar ${isRadialOpen ? 'radial-open-bg' : ''}`}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onPointerCancel={handlePointerUp}
      >
        {!isRadialOpen && (
          <>
            <svg 
              className="taskbar-silhouette" 
              xmlns="http://www.w3.org/2000/svg"
            >
              <defs>
                <mask id="laser-mask" maskUnits="userSpaceOnUse" x="0" y="0" width="100%" height="100%">
                  <path 
                    className="silhouette-path" 
                    fill="none" 
                    stroke="white"
                    strokeWidth="3"
                    d="M 0,0 A 32,32 0 0,1 32,32 L 32,52 A 28,28 0 0,0 60,80 L 444,80 A 28,28 0 0,0 472,52 L 472,32 A 32,32 0 0,1 504,0 Z"
                  />
                </mask>
              </defs>
              <path 
                className="silhouette-path" 
                fill="#000000" 
                stroke="rgba(255, 255, 255, 0.08)"
                strokeWidth="1"
                d="M 0,0 A 32,32 0 0,1 32,32 L 32,52 A 28,28 0 0,0 60,80 L 444,80 A 28,28 0 0,0 472,52 L 472,32 A 32,32 0 0,1 504,0 Z"
              />
              <foreignObject width="100%" height="100%" mask="url(#laser-mask)">
                <div className="taskbar-laser-container">
                  <div className="taskbar-laser" />
                </div>
              </foreignObject>
            </svg>
            <div className="taskbar-glass-layer" />
          </>
        )}

        <div className={`taskbar-content ${selectedNode !== null ? 'has-selected-node' : ''}`} style={{ perspective: '1200px', transformStyle: 'preserve-3d' }}>
          <AiOrbPanel 
            isRadialOpen={isRadialOpen} 
            onOrbClick={handleOrbClick}
            aiState={aiState}
          />

          {isInputOpen && !isRadialOpen && (
            <div className="ai-prompt-popup" onClick={e => e.stopPropagation()}>
              {/* Conversation History */}
              {committedMessages.length > 0 && (
                <div style={{ maxHeight: '200px', overflowY: 'auto', marginBottom: '8px', display: 'flex', flexDirection: 'column', gap: '6px' }}>
                  {committedMessages.map(msg => (
                    <div key={msg.id} style={{
                      padding: '6px 10px',
                      borderRadius: '8px',
                      fontSize: '13px',
                      lineHeight: '1.4',
                      background: msg.role === 'user' ? 'rgba(66, 133, 244, 0.15)' : 'rgba(255, 255, 255, 0.05)',
                      color: '#fff',
                      alignSelf: msg.role === 'user' ? 'flex-end' : 'flex-start',
                      maxWidth: '90%',
                      wordWrap: 'break-word',
                      whiteSpace: 'pre-wrap',
                    }}>
                      {msg.content}
                    </div>
                  ))}
                </div>
              )}

              {/* Streaming / Error / Thinking */}
              {aiState !== 'idle' && (
                <div style={{ padding: '4px', marginBottom: '4px' }}>
                  {aiState === 'thinking' && <div className="ai-spinner"></div>}
                  {errorMsg && <div className="ai-error-text" style={{ color: '#ff6b6b' }}>⚠ {errorMsg}</div>}
                  {aiState === 'speaking' && streamingText && (
                    <div className="ai-result-text" style={{
                      background: 'rgba(255, 255, 255, 0.05)',
                      padding: '6px 10px',
                      borderRadius: '8px',
                      whiteSpace: 'pre-wrap',
                      wordWrap: 'break-word',
                    }}>{streamingText}</div>
                  )}
                </div>
              )}

              {/* Input Row */}
              <div style={{ display: 'flex', width: '100%', alignItems: 'center', gap: '4px' }}>
                <input 
                  type="text"
                  className="ai-prompt-input"
                  placeholder={committedMessages.length > 0 ? 'Tiếp tục hội thoại...' : 'Hỏi AI...'}
                  value={prompt}
                  onChange={e => setPrompt(e.target.value)}
                  onKeyDown={handleAiKeyDown}
                  autoFocus
                  style={{ flex: 1 }}
                />
                {committedMessages.length > 0 && (
                  <button 
                    onClick={clearConversation}
                    title="Xoá hội thoại"
                    style={{ background: 'none', border: 'none', color: '#aaa', cursor: 'pointer', padding: '0 4px', fontSize: '12px' }}
                  >
                    🗑
                  </button>
                )}
                <button 
                  onClick={handleOrbClick}
                  style={{ background: 'none', border: 'none', color: '#aaa', cursor: 'pointer', padding: '0 8px', fontSize: '14px' }}
                >
                  ✕
                </button>
              </div>
            </div>
          )}
          
          {!isRadialOpen && (
            <div className="datetime-widget-container" style={{ transition: 'opacity 0.2s ease-out, transform 0.2s ease-out' }}>
              <DateTimeWidget />
            </div>
          )}
          


          {/* Orbital Satellites */}
          {isRadialOpen && (
            <div className="radial-orbitals-container" style={{ transformStyle: 'preserve-3d' }}>
              <div 
                ref={orbitalsContainerRef}
                className="radial-orbitals"
              >
                {SATELLITES.map((sat, i) => {
                  const isSelected = selectedNode === i;
                  const isDimmed = selectedNode !== null && selectedNode !== i;
                  
                  return (
                    <div 
                      key={sat.id} 
                      className="orbital-node-wrapper"
                      style={{ transform: `rotate(${i * 60}deg)` }}
                    >
                      {hoveredNode === i && selectedNode === null && <div className="connection-ray" />}
                      
                      <div 
                        className="orbital-node-container"
                        style={{
                          position: 'absolute',
                          top: 0,
                          left: 0,
                          width: 0,
                          height: 0,
                          transformStyle: 'preserve-3d',
                          transform: `translateY(-140px) rotate(-${i * 60}deg)`
                        }}
                      >
                        <div 
                          ref={el => { nodesRef.current[i] = el; }}
                          className={`orbital-node ${isSelected ? 'selected' : ''} ${isDimmed ? 'dimmed' : ''}`}
                          onMouseEnter={() => setHoveredNode(i)}
                          onMouseLeave={() => setHoveredNode(null)}
                          onClick={(e) => {
                            e.stopPropagation();
                            if (selectedNode === i) {
                              setSelectedNode(null);
                            } else {
                              setSelectedNode(i);
                            }
                          }}
                        >
                          <sat.icon size={20} color={isSelected ? sat.color : "rgba(255,255,255,0.8)"} />
                          
                          {(hoveredNode === i || isSelected) && (
                            <div className="orbital-label" style={{ color: sat.color }}>
                              {sat.label}
                            </div>
                          )}
                        </div>
                      </div>
                    </div>
                  );
                })}
              </div>

              {/* Placeholder Panel when a node is selected */}
              {selectedNode !== null && (
                <div ref={placeholderPanelRef} className="placeholder-panel">
                  <div className="placeholder-header">
                    <button className="icon-btn back-btn" onClick={() => setSelectedNode(null)} title="Back">
                      <ChevronLeft size={16} />
                    </button>
                    <span style={{ color: SATELLITES[selectedNode].color, display: 'flex', alignItems: 'center', gap: '8px', fontWeight: 600 }}>
                      {(() => {
                        const Icon = SATELLITES[selectedNode].icon;
                        return <Icon size={16} />;
                      })()}
                      {SATELLITES[selectedNode].label}
                    </span>
                    <div style={{ width: '24px' }}></div> {/* Spacer for centering */}
                  </div>
                  <div className="placeholder-body">
                    {renderPlaceholderContent(selectedNode)}
                  </div>
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
