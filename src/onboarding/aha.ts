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
  inert?(): boolean;
}

export interface AhaWatcher {
  start(): () => void;
  timerEligible(): boolean;
  onTimerStopped(payload: { taskId: number | string; minutes: number; title: string }): Promise<void>;
}

export const ahaKey = (userId: number | string): string =>
  `tmgr:onboarding:aha:${userId}`;

type Kind = 'timer' | 'agent';

const POLL_MS = 30_000;
const WINDOW_MS = 7 * 24 * 60 * 60 * 1000;

export function createAhaWatcher(deps: AhaDeps): AhaWatcher {
  const remembered: Record<Kind, boolean> = { timer: false, agent: false };

  const stored = (userId: number | string): Partial<Record<Kind, boolean>> => {
    try {
      const parsed = JSON.parse(deps.storage.getItem(ahaKey(userId)) ?? 'null');
      return parsed && typeof parsed === 'object' ? parsed : {};
    } catch {
      return {};
    }
  };

  const isShown = (userId: number | string, kind: Kind) =>
    remembered[kind] || !!stored(userId)[kind];

  const markShown = (userId: number | string, kind: Kind) => {
    remembered[kind] = true;
    try {
      deps.storage.setItem(ahaKey(userId), JSON.stringify({ ...stored(userId), [kind]: true }));
    } catch {
      return;
    }
  };

  const eligible = (kind: Kind, intent: string) => {
    const userId = deps.userId();
    if (deps.inert?.() || userId === null || deps.intent() !== intent || isShown(userId, kind)) return null;
    return userId;
  };

  return {
    timerEligible() {
      return eligible('timer', 'human') !== null;
    },

    start() {
      const userId = deps.userId();
      const intent = deps.intent();
      if (deps.inert?.() || userId === null || (intent !== 'agent' && intent !== 'human'))
        return () => {};
      const since = deps.intentAt() ?? deps.now();
      if (intent !== 'agent' || isShown(userId, 'agent')) return () => {};
      const expired = () => deps.now() - since >= WINDOW_MS;
      if (expired()) return () => {};

      let live = true;
      let busy = false;
      let sawPending = false;
      let cancelTimer: (() => void) | null = null;
      const stop = () => {
        live = false;
        cancelTimer?.();
        cancelTimer = null;
        off();
      };

      const poll = async () => {
        if (!live || busy || deps.visibility.hidden()) return;
        if (expired()) return stop();
        busy = true;
        let facts: Record<string, unknown> | null = null;
        try {
          facts = await deps.fetchFacts(['agent_runs_finished']);
        } catch {
          facts = null;
        }
        busy = false;
        if (!live) return;
        if (isShown(userId, 'agent')) return stop();
        if (facts?.agent_runs_finished) {
          const observedPending = sawPending;
          markShown(userId, 'agent');
          if (observedPending) deps.toast('First agent run logged. This is what TMGR does.');
          return stop();
        }
        if (facts) sawPending = true;
        cancelTimer = deps.setTimer(() => {
          cancelTimer = null;
          void poll();
        }, POLL_MS);
      };

      const off = deps.visibility.onChange(() => {
        cancelTimer?.();
        cancelTimer = null;
        void poll();
      });
      void poll();
      return stop;
    },

    async onTimerStopped(payload) {
      const userId = eligible('timer', 'human');
      if (userId === null) return;
      const since = deps.intentAt() ?? deps.now();
      if (deps.now() - since >= WINDOW_MS) return;
      let facts: Record<string, unknown>;
      try {
        facts = await deps.fetchFacts(['has_timer_minute'], payload.taskId);
      } catch {
        return;
      }
      if (!facts.has_timer_minute || isShown(userId, 'timer')) return;
      markShown(userId, 'timer');
      const minutes = Math.max(1, Math.round(payload.minutes));
      deps.toast(`Logged ${minutes}m on ${payload.title}. Your time stays with the task.`);
    },
  };
}
