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

  return {
    start() {
      const userId = deps.userId();
      if (deps.inert?.() || userId === null || deps.intent() !== 'agent' || isShown(userId, 'agent'))
        return () => {};
      const since = deps.intentAt() ?? deps.now();
      const expired = () => deps.now() - since >= WINDOW_MS;
      if (expired()) return () => {};

      let live = true;
      let busy = false;
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
        let facts: Record<string, unknown> = {};
        try {
          facts = await deps.fetchFacts(['agent_runs_finished']);
        } catch {
          facts = {};
        }
        busy = false;
        if (!live) return;
        if (facts.agent_runs_finished) {
          markShown(userId, 'agent');
          deps.toast('First agent run logged. This is what TMGR does.');
          return stop();
        }
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
      const userId = deps.userId();
      if (deps.inert?.() || userId === null || deps.intent() !== 'human' || isShown(userId, 'timer'))
        return;
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
