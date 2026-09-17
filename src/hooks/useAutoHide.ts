/**
 * useAutoHide.ts — Hook that listens to Tauri events and drives
 * spring-physics animation for show/hide.
 *
 * Instead of CSS keyframes, this uses JS spring simulation for
 * buttery smooth, iOS-like fluid motion with no discrete "steps".
 */

import { useEffect, useRef, useCallback } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { useSpringAnimation, SPRING_PRESETS } from './useSpringAnimation';

export type BarVisibility = 'hidden' | 'showing' | 'visible' | 'hiding';

export function useAutoHide(barRef: React.RefObject<HTMLDivElement | null>) {
  const visibilityRef = useRef<BarVisibility>('hidden');
  const { animate, reset } = useSpringAnimation();

  // Initialize spring states to "hidden" position
  useEffect(() => {
    reset({
      progress: 0,       
      scaleX: 1,       
      translateY: -100,     
      opacity: 0,
      bottomRadius: 28,  
    });

    if (barRef.current) {
      barRef.current.style.transform = 'scaleX(1) scaleY(1) translateY(-100px)';
      barRef.current.style.opacity = '0';
      barRef.current.style.borderRadius = '0 0 28px 28px';
      barRef.current.style.pointerEvents = 'none';
      const content = barRef.current.querySelector('.taskbar-content') as HTMLElement;
      if (content) content.style.opacity = '0';
      
      const laserContainer = barRef.current.querySelector('.taskbar-laser-container') as HTMLElement;
      const laser = barRef.current.querySelector('.taskbar-laser') as HTMLElement;
      if (laserContainer && laser) {
        laserContainer.style.opacity = '0';
        laser.style.animationPlayState = 'paused';
      }
    }
  }, [reset, barRef]);

  const applyStyles = useCallback((values: Record<string, number>, velocities: Record<string, number>) => {
    if (!barRef.current) return;
    const el = barRef.current;
    
    // Calculate Content Opacity: fades out early when translateY is pulling up
    let contentOp = 1.0;
    if (values.translateY < -30) {
      contentOp = Math.max(0, 1 - Math.abs(values.translateY + 30) / 70);
    }
    
    // --- WATER DROP ELASTICITY (Squash and Stretch) ---
    // Use velocity of translateY to deform the shape.
    // If moving very fast vertically, stretch Y and squash X.
    const velY = velocities.translateY || 0;
    
    // Max stretch is 1.4, max squash is 0.7. Scale deformation by velocity magnitude.
    const deformation = Math.min(Math.abs(velY) / 1500, 0.4);
    
    // When moving up/down rapidly, stretch Y, squash X
    const stretchY = 1.0 + deformation;
    const squashX = values.scaleX * (1.0 - deformation * 0.5); // Less squash than stretch to preserve volume feeling
    
    el.style.transform = `scaleX(${squashX}) scaleY(${stretchY}) translateY(${values.translateY || 0}px)`;
    el.style.opacity = `${Math.max(0, Math.min(1, values.opacity))}`;
    el.style.borderRadius = `0 0 ${values.bottomRadius}px ${values.bottomRadius}px`;
    
    const content = el.querySelector('.taskbar-content') as HTMLElement;
    if (content) {
      content.style.opacity = `${contentOp}`;
    }

    const laserContainer = el.querySelector('.taskbar-laser-container') as HTMLElement;
    const laser = el.querySelector('.taskbar-laser') as HTMLElement;
    if (laserContainer && laser) {
      if (values.opacity === 0) {
        laserContainer.style.opacity = '0';
        laser.style.animationPlayState = 'paused';
      } else {
        laserContainer.style.opacity = '1';
        laser.style.animationPlayState = 'running';
      }
    }
  }, [barRef]);

  const showBar = useCallback(() => {
    if (visibilityRef.current === 'visible' || visibilityRef.current === 'showing') return;
    visibilityRef.current = 'showing';
    window.dispatchEvent(new CustomEvent('taskbar-visibility', { detail: { isVisible: true } }));

    // Tell Rust we're in Bar mode (only the bar area accepts input)
    invoke('set_interaction_mode', { mode: 'bar' }).catch(console.error);

    if (barRef.current) {
      barRef.current.style.pointerEvents = 'auto';
    }

    animate(
      {
        scaleX: 1,
        translateY: 0,
        opacity: 1,
        bottomRadius: 28,
      },
      SPRING_PRESETS.fluid,
      applyStyles,
      () => {
        if (visibilityRef.current === 'showing') {
          visibilityRef.current = 'visible';
        }
      }
    );
  }, [animate, applyStyles, barRef]);

  const isLockedRef = useRef(false);
  const setIsLocked = useCallback((locked: boolean) => {
    isLockedRef.current = locked;
  }, []);

  const hideBar = useCallback(() => {
    if (isLockedRef.current) return;
    if (visibilityRef.current === 'hidden' || visibilityRef.current === 'hiding') return;
    visibilityRef.current = 'hiding';

    // Slide up instead of shrinking
    animate(
      {
        scaleX: 1, 
        translateY: -100,
        opacity: 0,   
        bottomRadius: 28,
      },
      SPRING_PRESETS.gentle,
      applyStyles,
      () => {
        if (visibilityRef.current === 'hiding') {
          visibilityRef.current = 'hidden';
          window.dispatchEvent(new CustomEvent('taskbar-visibility', { detail: { isVisible: false } }));
          
          // Tell Rust we're hidden (click-through everything)
          invoke('set_interaction_mode', { mode: 'hidden' }).catch(console.error);
          
          if (barRef.current) {
            barRef.current.style.pointerEvents = 'none';
          }
        }
      }
    );
  }, [animate, applyStyles, barRef]);

  // Listen for Tauri events
  useEffect(() => {
    let showUnlisten: (() => void) | null = null;
    let hideUnlisten: (() => void) | null = null;

    const setup = async () => {
      showUnlisten = await listen('show-bar', () => showBar());
      hideUnlisten = await listen('hide-bar', () => hideBar());
      
      // Check initial state in case mouse is already in hit zone
      const shouldShow = await invoke<boolean>('check_initial_state');
      if (shouldShow) {
        showBar();
      }
    };

    setup();

    return () => {
      showUnlisten?.();
      hideUnlisten?.();
    };
  }, [showBar, hideBar]);

  return { visibility: visibilityRef, setIsLocked, stopAnimation: stop };
}
