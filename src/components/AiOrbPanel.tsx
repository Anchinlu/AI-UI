import { useEffect, useRef, useState } from 'react';
import './AiOrbPanel.css';

// 3D Engine Math
const PHI = (1 + Math.sqrt(5)) / 2;

// 12 vertices of a regular icosahedron
const VERTICES = [
  [-1,  PHI, 0], [ 1,  PHI, 0], [-1, -PHI, 0], [ 1, -PHI, 0],
  [0, -1,  PHI], [0,  1,  PHI], [0, -1, -PHI], [0,  1, -PHI],
  [ PHI, 0, -1], [ PHI, 0,  1], [-PHI, 0, -1], [-PHI, 0,  1]
];

// Calculate edges once (30 edges)
const EDGES: [number, number][] = [];
for (let i = 0; i < VERTICES.length; i++) {
  for (let j = i + 1; j < VERTICES.length; j++) {
    const dx = VERTICES[i][0] - VERTICES[j][0];
    const dy = VERTICES[i][1] - VERTICES[j][1];
    const dz = VERTICES[i][2] - VERTICES[j][2];
    const distSq = dx * dx + dy * dy + dz * dz;
    // Edge length is 2, distSq is 4
    if (Math.abs(distSq - 4.0) < 0.1) {
      EDGES.push([i, j]);
    }
  }
}

export type AiState = 'idle' | 'listening' | 'thinking' | 'speaking' | 'error';

export function AiOrbPanel({ 
  isRadialOpen,
  setIsInputOpen,
  aiState
}: { 
  isRadialOpen?: boolean;
  setIsInputOpen?: React.Dispatch<React.SetStateAction<boolean>>;
  aiState: AiState;
}) {
  const svgRef = useRef<SVGSVGElement>(null);
  const linesRef = useRef<(SVGLineElement | null)[]>([]);
  const coreRef = useRef<SVGCircleElement>(null);
  
  // Mutable animation state to bypass React re-renders for max FPS
  const anim = useRef({
    rx: 0, ry: 0, rz: 0,
    speed: 1.0, scale: 15,
    glowOpacity: 0.5
  });

  const [isVisible, setIsVisible] = useState(true);

  const handleOrbClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (isRadialOpen) return;

    if (setIsInputOpen) {
      setIsInputOpen(prev => !prev);
    }
  };

  useEffect(() => {
    const handleVis = (e: Event) => {
      const customEvent = e as CustomEvent;
      setIsVisible(customEvent.detail.isVisible);
    };
    window.addEventListener('taskbar-visibility', handleVis);
    return () => window.removeEventListener('taskbar-visibility', handleVis);
  }, []);

  useEffect(() => {
    let reqId: number;
    let lastTime = performance.now();

    const loop = (time: number) => {
      if (!isVisible) {
        // Just loop slowly or don't request frame at all? 
        // If we don't request, we need a way to restart it when isVisible becomes true.
        // Since isVisible is in the dependency array, this useEffect will re-run when it changes!
        return; 
      }

      const dt = (time - lastTime) / 1000;
      lastTime = time;

      // Update state targets based on aiState
      let targetSpeed = 0.8;
      let targetScale = 16;
      
      if (aiState === 'listening') {
        targetSpeed = 4.0;
        targetScale = 18.5;
      } else if (aiState === 'thinking') {
        targetSpeed = 0.3;
        targetScale = 16 + Math.sin(time * 0.003) * 1.5; // Breathing scale (smaller)
      } else if (aiState === 'speaking') {
        targetSpeed = 1.5;
        targetScale = 17 + Math.sin(time * 0.02) * 1.5; // Fast pulse sync with voice (smaller)
      } else if (aiState === 'error') {
        targetSpeed = 0.0;
        targetScale = 16 + (Math.random() - 0.5) * 2; // Glitch/Jitter
      }
      
      // Smooth interpolation for fluid transitions
      anim.current.speed += (targetSpeed - anim.current.speed) * 0.05;
      anim.current.scale += (targetScale - anim.current.scale) * 0.1;

      // Rotate geometry
      anim.current.rx += 0.5 * anim.current.speed * dt;
      anim.current.ry += 0.7 * anim.current.speed * dt;
      anim.current.rz += 0.3 * anim.current.speed * dt;

      const sx = Math.sin(anim.current.rx), cx = Math.cos(anim.current.rx);
      const sy = Math.sin(anim.current.ry), cy = Math.cos(anim.current.ry);
      const sz = Math.sin(anim.current.rz), cz = Math.cos(anim.current.rz);

      for (let i = 0; i < EDGES.length; i++) {
        const [v1, v2] = EDGES[i];
        const p1 = VERTICES[v1], p2 = VERTICES[v2];

        const transform = (p: number[]) => {
          const x1 = p[0]*cy*cz - p[0]*sz*sx*sy - p[1]*sz*cx + p[2]*sy*cz + p[2]*sz*sx*cy;
          const y1 = p[0]*cy*sz + p[0]*cz*sx*sy + p[1]*cx*cz + p[2]*sy*sz - p[2]*cz*sx*cy;
          return { x: x1, y: y1 };
        };

        const t1 = transform(p1);
        const t2 = transform(p2);

        const s = anim.current.scale;
        const line = linesRef.current[i];
        if (line) {
          line.setAttribute('x1', (40 + t1.x * s).toString());
          line.setAttribute('y1', (40 + t1.y * s).toString());
          line.setAttribute('x2', (40 + t2.x * s).toString());
          line.setAttribute('y2', (40 + t2.y * s).toString());

          const opacity = Math.max(0.1, 1 - (aiState === 'thinking' ? 0.8 : 0));
          const strokeColor = `rgba(255, 255, 255, ${opacity * 0.6})`;
          line.setAttribute('stroke', strokeColor);
        }
      }

      if (coreRef.current) {
        let baseR = 2.5;
        if (aiState === 'speaking') baseR = 3.5 + Math.sin(time * 0.05) * 1.5;
        if (aiState === 'listening') baseR = 4.0;
        coreRef.current.setAttribute('r', baseR.toString());
        coreRef.current.setAttribute('fill', '#ffffff');
      }

      reqId = requestAnimationFrame(loop);
    };

    reqId = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(reqId);
  }, [aiState, isVisible]);

  const RAYS = Array.from({ length: 16 }).map((_, i) => {
    const angle = (i * 22.5) * (Math.PI / 180);
    const isLong = i % 2 === 0;
    const innerR = 4;
    const outerR = isLong ? 26 : 14;
    return {
      x1: 40 + innerR * Math.sin(angle),
      y1: 40 - innerR * Math.cos(angle),
      x2: 40 + outerR * Math.sin(angle),
      y2: 40 - outerR * Math.cos(angle),
      isLong
    };
  });

  return (
    <div 
      className={`ai-orb-panel ${isRadialOpen ? 'radial-open' : ''}`}
      onClick={handleOrbClick}
      title={`Current State: ${aiState}. Click to toggle input.`}
    >
      <svg ref={svgRef} width="80" height="80" viewBox="0 0 80 80" className="ai-orb-svg">
        <defs>
          <radialGradient id="ray-gradient" cx="50%" cy="50%" r="50%">
            <stop offset="0%" stopColor="rgba(255, 255, 255, 0.9)" />
            <stop offset="100%" stopColor="rgba(255, 255, 255, 0)" />
          </radialGradient>
        </defs>
        
        {/* Rays shooting outwards */}
        <g className={`core-rays ${aiState}`}>
           {RAYS.map((r, i) => (
             <line 
               key={i} 
               x1={r.x1} y1={r.y1} x2={r.x2} y2={r.y2} 
               stroke="url(#ray-gradient)" 
               strokeWidth={r.isLong ? "1" : "0.5"} 
               strokeLinecap="round" 
             />
           ))}
        </g>

        {/* Tiny Glowing Center Point */}
        <circle ref={coreRef} cx="40" cy="40" r="2.5" fill="#ffffff" style={{ filter: 'drop-shadow(0 0 4px #ffffff)' }} />

        {/* 3D Wireframe Lines */}
        {EDGES.map((_, i) => (
          <line key={i} ref={el => { linesRef.current[i] = el; }} strokeWidth="1.2" strokeLinecap="round" />
        ))}
      </svg>
    </div>
  );
}
