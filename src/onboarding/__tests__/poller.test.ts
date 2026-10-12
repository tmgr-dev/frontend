import { createFactsPoller, intervalFor } from '../poller';
import type { FactsPollerDeps } from '../poller';
import { createTourBus } from '@/tours/events';

const S = 1000;

const harness = (over: Record<string, unknown> = {}) => {
  let clock = 0;
  let hidden = false;
  const timers: Array<{ at: number; fn: () => void; live: boolean }> = [];
  const listeners = new Set<() => void>();
  const responses: Array<Record<string, unknown>> = [];
  const fetchFacts = jest.fn(async () => responses.shift() ?? {});
  const bus = createTourBus();
  const emitted: string[] = [];
  bus.on('agent.first_mcp_call', () => emitted.push('agent.first_mcp_call'));
  const deps: FactsPollerDeps = {
    fetchFacts,
    keys: ['mcp_first_call_at'],
    now: () => clock,
    setTimer: (fn, ms) => {
      const entry = { at: clock + ms, fn, live: true };
      timers.push(entry);
      return () => {
        entry.live = false;
      };
    },
    visibility: {
      hidden: () => hidden,
      onChange: (fn) => {
        listeners.add(fn);
        return () => listeners.delete(fn);
      },
    },
    bus,
    ...(over as Partial<FactsPollerDeps>),
  };
  const poller = createFactsPoller(deps);
  const advance = async (ms: number) => {
    const target = clock + ms;
    for (;;) {
      const due = timers
        .filter((t) => t.live && t.at <= target)
        .sort((a, b) => a.at - b.at)[0];
      if (!due) break;
      due.live = false;
      clock = due.at;
      due.fn();
      await flush();
    }
    clock = target;
  };
  return {
    poller,
    fetchFacts,
    responses,
    emitted,
    advance,
    setClock: (v: number) => (clock = v),
    setHidden: async (v: boolean) => {
      hidden = v;
      listeners.forEach((fn) => fn());
      await flush();
    },
    liveTimers: () => timers.filter((t) => t.live).length,
  };
};

const flush = async () => {
  for (let i = 0; i < 30; i++) await Promise.resolve();
};

describe('intervalFor', () => {
  it.each([
    [0, 5 * S],
    [59 * S, 5 * S],
    [60 * S, 15 * S],
    [299 * S, 15 * S],
    [300 * S, 30 * S],
    [599 * S, 30 * S],
    [600 * S, null],
    [900 * S, null],
  ])('elapsed %p -> %p', (elapsed, expected) => {
    expect(intervalFor(elapsed)).toBe(expected);
  });
});

describe('createFactsPoller', () => {
  it('polls immediately then every 5s in the first minute', async () => {
    const h = harness();
    h.poller.start();
    await flush();
    expect(h.fetchFacts).toHaveBeenCalledTimes(1);
    await h.advance(5 * S);
    expect(h.fetchFacts).toHaveBeenCalledTimes(2);
    await h.advance(5 * S);
    expect(h.fetchFacts).toHaveBeenCalledTimes(3);
  });

  it('slows to 15s after a minute and 30s after five', async () => {
    const h = harness();
    h.poller.start();
    await flush();
    await h.advance(60 * S);
    const at60 = h.fetchFacts.mock.calls.length;
    await h.advance(14 * S);
    expect(h.fetchFacts).toHaveBeenCalledTimes(at60);
    await h.advance(1 * S);
    expect(h.fetchFacts).toHaveBeenCalledTimes(at60 + 1);
    await h.advance(225 * S);
    const at300 = h.fetchFacts.mock.calls.length;
    await h.advance(29 * S);
    expect(h.fetchFacts).toHaveBeenCalledTimes(at300);
    await h.advance(1 * S);
    expect(h.fetchFacts).toHaveBeenCalledTimes(at300 + 1);
  });

  it('passes keys and task id to fetchFacts', async () => {
    const h = harness({ taskId: 7 });
    h.poller.start();
    await flush();
    expect(h.fetchFacts).toHaveBeenCalledWith(['mcp_first_call_at'], 7);
  });

  it('stops with exhausted after ten minutes', async () => {
    const onExhausted = jest.fn();
    const h = harness({ onExhausted });
    h.poller.start();
    await flush();
    await h.advance(700 * S);
    expect(h.poller.state.value).toBe('exhausted');
    expect(onExhausted).toHaveBeenCalledTimes(1);
    const calls = h.fetchFacts.mock.calls.length;
    await h.advance(100 * S);
    expect(h.fetchFacts).toHaveBeenCalledTimes(calls);
  });

  it('makes no requests while hidden and polls once on visible', async () => {
    const h = harness();
    await h.setHidden(true);
    h.poller.start();
    await flush();
    await h.advance(30 * S);
    expect(h.fetchFacts).not.toHaveBeenCalled();
    await h.setHidden(false);
    expect(h.fetchFacts).toHaveBeenCalledTimes(1);
  });

  it('pauses when the tab becomes hidden mid-poll', async () => {
    const h = harness();
    h.poller.start();
    await flush();
    await h.setHidden(true);
    await h.advance(30 * S);
    expect(h.fetchFacts).toHaveBeenCalledTimes(1);
    await h.setHidden(false);
    expect(h.fetchFacts).toHaveBeenCalledTimes(2);
  });

  it('goes exhausted on visible when hidden past the window', async () => {
    const h = harness();
    await h.setHidden(true);
    h.poller.start();
    await h.advance(700 * S);
    await h.setHidden(false);
    expect(h.poller.state.value).toBe('exhausted');
    expect(h.fetchFacts).not.toHaveBeenCalled();
  });

  it('is done and emits agent.first_mcp_call when the fact flips', async () => {
    const onFacts = jest.fn();
    const h = harness({ onFacts });
    h.responses.push({}, { mcp_first_call_at: '2026-10-12T10:00:00Z' });
    h.poller.start();
    await flush();
    await h.advance(5 * S);
    expect(h.poller.state.value).toBe('done');
    expect(h.emitted).toEqual(['agent.first_mcp_call']);
    expect(onFacts).toHaveBeenCalledTimes(2);
    expect(h.liveTimers()).toBe(0);
    await h.advance(60 * S);
    expect(h.fetchFacts).toHaveBeenCalledTimes(2);
  });

  it('does not emit for other keys', async () => {
    const h = harness({ keys: ['agent_runs_finished'] });
    h.responses.push({ agent_runs_finished: 1 });
    h.poller.start();
    await flush();
    expect(h.poller.state.value).toBe('done');
    expect(h.emitted).toEqual([]);
  });

  it('waits for every key to be truthy', async () => {
    const h = harness({ keys: ['a', 'b'] });
    h.responses.push({ a: 1, b: 0 }, { a: 1, b: 2 });
    h.poller.start();
    await flush();
    expect(h.poller.state.value).toBe('polling');
    await h.advance(5 * S);
    expect(h.poller.state.value).toBe('done');
  });

  it('stalls at 180s, keeps polling, and checkAgain restarts the 5s phase', async () => {
    const onStall = jest.fn();
    const h = harness({ onStall });
    h.poller.start();
    await flush();
    await h.advance(179 * S);
    expect(onStall).not.toHaveBeenCalled();
    await h.advance(1 * S);
    expect(onStall).toHaveBeenCalledTimes(1);
    expect(h.poller.state.value).toBe('stalled');
    const before = h.fetchFacts.mock.calls.length;
    h.poller.checkAgain();
    await flush();
    expect(h.poller.state.value).toBe('polling');
    expect(h.fetchFacts).toHaveBeenCalledTimes(before + 1);
    await h.advance(5 * S);
    expect(h.fetchFacts).toHaveBeenCalledTimes(before + 2);
    await h.advance(175 * S);
    expect(onStall).toHaveBeenCalledTimes(2);
  });

  it('checkAgain revives an exhausted poller', async () => {
    const h = harness();
    h.poller.start();
    await flush();
    await h.advance(700 * S);
    expect(h.poller.state.value).toBe('exhausted');
    h.poller.checkAgain();
    await flush();
    expect(h.poller.state.value).toBe('polling');
  });

  it('stop cancels timers and ignores in-flight results', async () => {
    let resolve: (v: Record<string, unknown>) => void = () => {};
    const fetchFacts = jest.fn(() => new Promise<Record<string, unknown>>((r) => (resolve = r)));
    const h = harness({ fetchFacts });
    h.poller.start();
    h.poller.stop();
    resolve({ mcp_first_call_at: 'x' });
    await flush();
    expect(h.emitted).toEqual([]);
    expect(h.liveTimers()).toBe(0);
    await h.advance(60 * S);
    expect(fetchFacts).toHaveBeenCalledTimes(1);
  });

  it('survives a failed request and keeps polling', async () => {
    const fetchFacts = jest
      .fn()
      .mockRejectedValueOnce(new Error('boom'))
      .mockResolvedValue({ mcp_first_call_at: 'x' });
    const h = harness({ fetchFacts });
    h.poller.start();
    await flush();
    await h.advance(5 * S);
    expect(h.poller.state.value).toBe('done');
  });

  it('never overlaps requests', async () => {
    let resolve: (v: Record<string, unknown>) => void = () => {};
    const fetchFacts = jest.fn(() => new Promise<Record<string, unknown>>((r) => (resolve = r)));
    const h = harness({ fetchFacts });
    h.poller.start();
    await h.advance(20 * S);
    expect(fetchFacts).toHaveBeenCalledTimes(1);
    resolve({});
    await flush();
    await h.advance(5 * S);
    expect(fetchFacts).toHaveBeenCalledTimes(2);
  });

  it('is exhausted without requests when inert (local mode)', async () => {
    const onExhausted = jest.fn();
    const h = harness({ inert: () => true, onExhausted });
    h.poller.start();
    await flush();
    expect(h.poller.state.value).toBe('exhausted');
    expect(onExhausted).toHaveBeenCalledTimes(1);
    expect(h.fetchFacts).not.toHaveBeenCalled();
    h.poller.checkAgain();
    await flush();
    expect(h.fetchFacts).not.toHaveBeenCalled();
  });
});
