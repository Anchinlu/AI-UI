/**
 * aiState.ts — AI state management.
 *
 * PLACEHOLDER — Phase 4 implementation.
 * Will manage AI states: idle | listening | thinking | speaking | error
 * and expose setAiState() for testing before real AI integration (Phase 6).
 */

export type AiState = 'idle' | 'listening' | 'thinking' | 'speaking' | 'error';

// Placeholder — will be implemented with proper state management in Phase 4
export const defaultAiState: AiState = 'idle';
