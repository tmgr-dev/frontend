import { ahaKey, createAhaWatcher } from '../aha';
import type { AhaDeps } from '../aha';

const DAY = 24 * 60 * 60 * 1000;
const flush = async () => {
  for (let i = 0; i < 30; i++) await Promise.resolve();
};

const harness = (over: Partial<AhaDeps> = {}) => {
  let clock = 1_000_000;
  let hidden = false;
  const data = new Map<string, string>();
  const timers: Array<{ at: number; fn: () => void; live: boolean }> = [];
  const listeners = new Set<() => void>();
  const toast = jest.fn();
  const fetchFacts = jest.fn().mockResolvedValue({});
  const deps: AhaDeps = {
    userId: () => 42,
    intent: () => 'human',
    intentAt: () => clock,
    now: () => clock,
    storage: { getItem: (k) => data.get(k) ?? null, setItem: (k, v) => void data.set(k, v) },
    fetchFacts,
    toast,
    visibility: {
      hidden: () => hidden,
      onChange: (fn) => {
        listeners.add(fn);
        return () => listeners.delete(fn);
      },
    },
    setTimer: (fn, ms) => {
      const entry = { at: clock + ms, fn, live: true };
      timers.push(entry);
      return () => {
        entry.live = false;
      };
    },
    ...over,
  };
  const watcher = createAhaWatcher(deps);
  const advance = async (ms: number) => {
    const target = clock + ms;
    for (;;) {
      const due = timers.filter((t) => t.live && t.at <= target).sort((a, b) => a.at - b.at)[0];
      if (!due) break;
      due.live = false;
      clock = due.at;
      due.fn();
      await flush();
    }
    clock = target;
  };
  return {
    watcher,
    toast,
    fetchFacts,
    data,
    advance,
    setHidden: async (v: boolean) => {
      hidden = v;
      listeners.forEach((fn) => fn());
      await flush();
    },
    live: () => timers.filter((t) => t.live).length,
  };
};

describe('human timer aha', () => {
  it('asks the server once for has_timer_minute on that task and toasts', async () => {
    const h = harness();
    h.fetchFacts.mockResolvedValue({ has_timer_minute: true });
    await h.watcher.onTimerStopped({ taskId: 5, minutes: 2, title: 'Plan your week' });
    expect(h.fetchFacts).toHaveBeenCalledWith(['has_timer_minute'], 5);
    expect(h.toast).toHaveBeenCalledWith('Logged 2m on Plan your week. Your time stays with the task.');
    expect(JSON.parse(h.data.get(ahaKey(42))!)).toMatchObject({ timer: true });
  });

  it('does not toast when the fact is false, and may try again later', async () => {
    const h = harness();
    await h.watcher.onTimerStopped({ taskId: 5, minutes: 0, title: 'x' });
    expect(h.toast).not.toHaveBeenCalled();
    h.fetchFacts.mockResolvedValue({ has_timer_minute: true });
    await h.watcher.onTimerStopped({ taskId: 5, minutes: 1, title: 'x' });
    expect(h.toast).toHaveBeenCalledTimes(1);
  });

  it('dedupes once the flag is stored, even across watchers', async () => {
    const h = harness();
    h.fetchFacts.mockResolvedValue({ has_timer_minute: true });
    await h.watcher.onTimerStopped({ taskId: 5, minutes: 1, title: 'x' });
    await h.watcher.onTimerStopped({ taskId: 6, minutes: 3, title: 'y' });
    expect(h.toast).toHaveBeenCalledTimes(1);
    expect(h.fetchFacts).toHaveBeenCalledTimes(1);
  });

  it.each(['agent', 'skipped', null])('ignores timer stops for intent %p', async (intent) => {
    const h = harness({ intent: () => intent });
    await h.watcher.onTimerStopped({ taskId: 5, minutes: 1, title: 'x' });
    expect(h.fetchFacts).not.toHaveBeenCalled();
  });

  it('ignores a signed-out user and a failed request', async () => {
    const signedOut = harness({ userId: () => null });
    await signedOut.watcher.onTimerStopped({ taskId: 5, minutes: 1, title: 'x' });
    expect(signedOut.fetchFacts).not.toHaveBeenCalled();
    const h = harness();
    h.fetchFacts.mockRejectedValue(new Error('net'));
    await expect(h.watcher.onTimerStopped({ taskId: 5, minutes: 1, title: 'x' })).resolves.toBeUndefined();
    expect(h.toast).not.toHaveBeenCalled();
  });

  it('is inert when told so', async () => {
    const h = harness({ inert: () => true });
    await h.watcher.onTimerStopped({ taskId: 5, minutes: 1, title: 'x' });
    const stop = h.watcher.start();
    await flush();
    expect(h.fetchFacts).not.toHaveBeenCalled();
    stop();
  });
});

describe('agent aha poller', () => {
  const agent = { intent: () => 'agent' as string | null };

  it('polls every 30s and toasts once when a run finished', async () => {
    const h = harness(agent);
    h.watcher.start();
    await flush();
    expect(h.fetchFacts).toHaveBeenCalledWith(['agent_runs_finished']);
    await h.advance(30_000);
    expect(h.fetchFacts).toHaveBeenCalledTimes(2);
    h.fetchFacts.mockResolvedValue({ agent_runs_finished: 1 });
    await h.advance(30_000);
    expect(h.toast).toHaveBeenCalledWith('First agent run logged. This is what TMGR does.');
    expect(h.live()).toBe(0);
    await h.advance(120_000);
    expect(h.fetchFacts).toHaveBeenCalledTimes(3);
  });

  it('does not poll when already shown', async () => {
    const h = harness(agent);
    h.data.set(ahaKey(42), JSON.stringify({ agent: true }));
    h.watcher.start();
    await flush();
    expect(h.fetchFacts).not.toHaveBeenCalled();
  });

  it('does not poll for human or unanswered intent', async () => {
    for (const intent of ['human', null, 'skipped']) {
      const h = harness({ intent: () => intent });
      h.watcher.start();
      await flush();
      expect(h.fetchFacts).not.toHaveBeenCalled();
    }
  });

  it('pauses while hidden and polls once on visible', async () => {
    const h = harness(agent);
    await h.setHidden(true);
    h.watcher.start();
    await flush();
    await h.advance(120_000);
    expect(h.fetchFacts).not.toHaveBeenCalled();
    await h.setHidden(false);
    expect(h.fetchFacts).toHaveBeenCalledTimes(1);
  });

  it('stops seven days after the intent was set', async () => {
    const h = harness(agent);
    h.watcher.start();
    await flush();
    await h.advance(7 * DAY + 60_000);
    const calls = h.fetchFacts.mock.calls.length;
    await h.advance(DAY);
    expect(h.fetchFacts).toHaveBeenCalledTimes(calls);
    expect(h.live()).toBe(0);
  });

  it('does not start when the intent is already older than seven days', async () => {
    const h = harness({ ...agent, intentAt: () => 1_000_000 - 8 * DAY });
    h.watcher.start();
    await flush();
    expect(h.fetchFacts).not.toHaveBeenCalled();
  });

  it('the returned disposer cancels everything', async () => {
    const h = harness(agent);
    const stop = h.watcher.start();
    await flush();
    stop();
    expect(h.live()).toBe(0);
    await h.advance(60_000);
    expect(h.fetchFacts).toHaveBeenCalledTimes(1);
  });

  it('survives storage failures', async () => {
    const h = harness({
      ...agent,
      storage: {
        getItem: () => {
          throw new Error('denied');
        },
        setItem: () => {
          throw new Error('denied');
        },
      },
    });
    h.watcher.start();
    await flush();
    h.fetchFacts.mockResolvedValue({ agent_runs_finished: 1 });
    await h.advance(30_000);
    expect(h.toast).toHaveBeenCalledTimes(1);
  });

  it('marks a fact that is already true on first look as shown, silently', async () => {
    const h = harness(agent);
    h.fetchFacts.mockResolvedValue({ agent_runs_finished: 1 });
    h.watcher.start();
    await flush();
    expect(h.toast).not.toHaveBeenCalled();
    expect(JSON.parse(h.data.get(ahaKey(42))!)).toMatchObject({ agent: true });
    expect(h.live()).toBe(0);
  });

  it('does not treat a failed first request as having seen the fact false', async () => {
    const h = harness(agent);
    h.fetchFacts.mockRejectedValueOnce(new Error('net'));
    h.watcher.start();
    await flush();
    h.fetchFacts.mockResolvedValue({ agent_runs_finished: 1 });
    await h.advance(30_000);
    expect(h.toast).not.toHaveBeenCalled();
    expect(JSON.parse(h.data.get(ahaKey(42))!)).toMatchObject({ agent: true });
  });

  it('does not toast when another tab marked it shown while the request was in flight', async () => {
    const h = harness(agent);
    h.watcher.start();
    await flush();
    h.fetchFacts.mockImplementation(async () => {
      h.data.set(ahaKey(42), JSON.stringify({ agent: true }));
      return { agent_runs_finished: 1 };
    });
    await h.advance(30_000);
    expect(h.toast).not.toHaveBeenCalled();
    expect(h.live()).toBe(0);
  });
});

describe('timer aha window', () => {
  it('does not toast after seven days since first seen', async () => {
    const h = harness({ intentAt: () => 1_000_000 - 8 * DAY });
    h.fetchFacts.mockResolvedValue({ has_timer_minute: true });
    await h.watcher.onTimerStopped({ taskId: 5, minutes: 1, title: 'x' });
    expect(h.fetchFacts).not.toHaveBeenCalled();
    expect(h.toast).not.toHaveBeenCalled();
  });

  it('reports whether a timer stop is worth looking at', () => {
    expect(harness().watcher.timerEligible()).toBe(true);
    expect(harness({ intent: () => 'agent' }).watcher.timerEligible()).toBe(false);
    expect(harness({ intent: () => null }).watcher.timerEligible()).toBe(false);
    expect(harness({ intent: () => 'skipped' }).watcher.timerEligible()).toBe(false);
    expect(harness({ inert: () => true }).watcher.timerEligible()).toBe(false);
    expect(harness({ userId: () => null }).watcher.timerEligible()).toBe(false);
  });

  it('records the first-seen moment as soon as a human or agent session starts', () => {
    const intentAt = jest.fn(() => 1_000_000);
    harness({ intentAt }).watcher.start();
    expect(intentAt).toHaveBeenCalled();
    const skipped = jest.fn(() => 1_000_000);
    harness({ intent: () => 'skipped', intentAt: skipped }).watcher.start();
    expect(skipped).not.toHaveBeenCalled();
  });
});
