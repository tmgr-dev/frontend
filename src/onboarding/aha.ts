export interface AhaDeps {
  userId: () => number | string | null;
  intent: () => string | null;
  intentAt: () => number | null;
  now(): number;
  storage: { getItem(k: string): string | null; setItem(k: string, v: string): void };
  fetchFacts(keys: string[], taskId?: number | string): Promise<Record<string, unknown>>;
  toast(message: string): void;
  visibility: { hidden(): boolean; onChange(fn: () => void): () => void };
  setTimer(fn: () => void, ms: number): () => void;
}

export interface AhaWatcher {
  start(): () => void;
  onTimerStopped(payload: { taskId: number | string; minutes: number; title: string }): Promise<void>;
}

export const ahaKey = (userId: number | string): string =>
  `tmgr:onboarding:aha:${userId}`;

export function createAhaWatcher(_deps: AhaDeps): AhaWatcher {
  return {
    start: () => () => {},
    async onTimerStopped() {},
  };
}
