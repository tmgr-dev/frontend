import type { Ref } from 'vue';
import { ref } from 'vue';

export type PollerState = 'polling' | 'stalled' | 'exhausted' | 'done';

export type Facts = Record<string, unknown>;

export interface FactsPollerDeps {
  fetchFacts(keys: string[], taskId?: number | string): Promise<Facts>;
  keys: string[];
  taskId?: number | string;
  now(): number;
  setTimer(fn: () => void, ms: number): () => void;
  visibility: { hidden(): boolean; onChange(fn: () => void): () => void };
  onFacts?(facts: Facts): void;
  stallAfterMs?: number;
  onStall?(): void;
  onExhausted?(): void;
}

export interface FactsPoller {
  start(): void;
  stop(): void;
  checkAgain(): void;
  state: Ref<PollerState>;
}

export function intervalFor(_elapsedMs: number): number | null {
  throw new Error('not implemented');
}

export function createFactsPoller(_deps: FactsPollerDeps): FactsPoller {
  return {
    start() {},
    stop() {},
    checkAgain() {},
    state: ref<PollerState>('done'),
  };
}
