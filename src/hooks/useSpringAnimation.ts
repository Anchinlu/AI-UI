/**
 * useSpringAnimation.ts — iOS-style spring physics animation engine.
 *
 * Uses requestAnimationFrame + spring physics simulation to create
 * buttery smooth, fluid animations like iOS Dynamic Island.
 *
 * Spring model: critically-damped spring (no overshoot oscillation,
 * but with the organic "settling" feel of real physics).
 *
 * Only drives `transform` and `opacity` — compositor-friendly.
 */

import { useRef, useCallback } from 'react';

interface SpringConfig {
  /** Spring stiffness — higher = snappier (iOS typically 300-500) */
  stiffness: number;
  /** Damping — higher = less oscillation (iOS typically 20-35) */
  damping: number;
  /** Mass — higher = more inertia (iOS typically 0.8-1.2) */
  mass: number;
}

interface SpringState {
  value: number;
  velocity: number;
}

/** iOS-like spring configs */
export const SPRING_PRESETS = {
  /** Snappy, minimal overshoot — like iOS notification */
  snappy: { stiffness: 400, damping: 30, mass: 0.8 },
  /** Fluid, slight bounce — like Dynamic Island expand */
  fluid: { stiffness: 280, damping: 24, mass: 1.0 },
  /** Gentle settle — like Dynamic Island collapse */
  gentle: { stiffness: 200, damping: 22, mass: 1.0 },
} as const;

/**
 * Simulate one step of a spring physics system.
 * Returns new state { value, velocity }.
 */
function springStep(
  state: SpringState,
  target: number,
  config: SpringConfig,
  dt: number
): SpringState {
  const { stiffness, damping, mass } = config;
  
  // Spring force: F = -k * displacement
  const displacement = state.value - target;
  const springForce = -stiffness * displacement;
  
  // Damping force: F = -c * velocity
  const dampingForce = -damping * state.velocity;
  
  // Acceleration: a = F / m
  const acceleration = (springForce + dampingForce) / mass;
  
  // Semi-implicit Euler integration
  const newVelocity = state.velocity + acceleration * dt;
  const newValue = state.value + newVelocity * dt;
  
  return { value: newValue, velocity: newVelocity };
}

/**
 * Check if spring has settled (close enough to target with low velocity).
 */
function isSettled(state: SpringState, target: number): boolean {
  return Math.abs(state.value - target) < 0.001 && Math.abs(state.velocity) < 0.01;
}

/**
 * Hook that provides a spring-physics animation engine.
 * Call `animate()` to start a spring animation toward a target.
 * The provided `onFrame` callback receives interpolated values each frame.
 */
export function useSpringAnimation() {
  const frameRef = useRef<number>(0);
  const statesRef = useRef<Map<string, SpringState>>(new Map());
  
  /**
   * Animate multiple properties simultaneously using spring physics.
   * 
   * @param targets - Map of property name to target value (0-1 range)
   * @param config - Spring configuration
   * @param onFrame - Called each frame with current values and velocities
   * @param onComplete - Called when animation settles
   */
  const animate = useCallback((
    targets: Record<string, number>,
    config: SpringConfig,
    onFrame: (values: Record<string, number>, velocities: Record<string, number>) => void,
    onComplete?: () => void
  ) => {
    // Cancel any running animation
    if (frameRef.current) {
      cancelAnimationFrame(frameRef.current);
    }
    
    // Initialize states for properties that don't exist yet
    for (const key of Object.keys(targets)) {
      if (!statesRef.current.has(key)) {
        statesRef.current.set(key, { value: 0, velocity: 0 });
      }
    }
    
    let lastTime = performance.now();
    
    const tick = (now: number) => {
      // Cap dt to avoid spiral of death on lag spikes
      const dt = Math.min((now - lastTime) / 1000, 0.064);
      lastTime = now;
      
      let allSettled = true;
      const currentValues: Record<string, number> = {};
      const currentVelocities: Record<string, number> = {};
      
      for (const [key, target] of Object.entries(targets)) {
        let state = statesRef.current.get(key)!;
        
        if (!isSettled(state, target)) {
          state = springStep(state, target, config, dt);
          statesRef.current.set(key, state);
          allSettled = false;
        } else {
          // Snap to target when settled
          state = { value: target, velocity: 0 };
          statesRef.current.set(key, state);
        }
        
        currentValues[key] = state.value;
        currentVelocities[key] = state.velocity;
      }
      
      onFrame(currentValues, currentVelocities);
      
      if (allSettled) {
        frameRef.current = 0;
        onComplete?.();
      } else {
        frameRef.current = requestAnimationFrame(tick);
      }
    };
    
    frameRef.current = requestAnimationFrame(tick);
  }, []);
  
  /** Stop any running animation */
  const stop = useCallback(() => {
    if (frameRef.current) {
      cancelAnimationFrame(frameRef.current);
      frameRef.current = 0;
    }
  }, []);
  
  /** Reset all spring states (e.g., for initial hidden state) */
  const reset = useCallback((values: Record<string, number>) => {
    for (const [key, value] of Object.entries(values)) {
      statesRef.current.set(key, { value, velocity: 0 });
    }
  }, []);
  
  return { animate, stop, reset };
}
