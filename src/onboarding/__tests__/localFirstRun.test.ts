import { emptyOnboarding } from '@/tours/store';
import {
  LOCAL_FIRST_RUN_SEQUENCE,
  isFreshLocalInstall,
  readFreshMarker,
  runLocalFirstRun,
  shouldRunLocalFirstRun,
  writeFreshMarker,
  type FreshMarker,
  type LocalFirstRunDeps,
  type MarkerStorage,
} from '../localFirstRun';

const MARKER: FreshMarker = { code: 'my-workspace', at: '2026-10-12T10:00:00.000Z' };

const memory = (): MarkerStorage & { map: Map<string, string> } => {
  const map = new Map<string, string>();
  return {
    map,
    getItem: (key) => map.get(key) ?? null,
    setItem: (key, value) => void map.set(key, value),
    removeItem: (key) => void map.delete(key),
  };
};

describe('isFreshLocalInstall', () => {
  it.each([
    [false, 0, null, true],
    [true, 0, null, false],
    [false, 1, null, false],
    [false, 0, JSON.stringify({ starter_seeded_at: '2026-10-12T00:00:00Z' }), false],
    [false, 0, JSON.stringify({ intent: 'human' }), true],
    [false, 0, '{not json', true],
  ])('profile=%s workspaces=%s onboarding=%s -> %s', (hasProfile, workspaceCount, onboardingRaw, expected) => {
    expect(isFreshLocalInstall({ hasProfile, workspaceCount, onboardingRaw })).toBe(expected);
  });
});

describe('fresh marker storage', () => {
  it('round-trips and treats junk as none', () => {
    const storage = memory();
    expect(readFreshMarker(storage)).toBeNull();
    writeFreshMarker(storage, 'w', new Date(MARKER.at));
    expect(readFreshMarker(storage)).toEqual({ code: 'w', at: MARKER.at });
    storage.setItem('tmgr:onboarding:local-fresh', '{"code":1}');
    expect(readFreshMarker(storage)).toBeNull();
  });
});

describe('shouldRunLocalFirstRun', () => {
  const base = { localOnly: true, mainWindow: true, marker: MARKER, activeCode: 'my-workspace', state: emptyOnboarding() };

  it('runs for a local main window with a marker and a pristine state', () => {
    expect(shouldRunLocalFirstRun(base)).toBe(true);
  });

  it.each([
    ['not local', { localOnly: false }],
    ['a detached window', { mainWindow: false }],
    ['no marker', { marker: null }],
    ['another active workspace', { activeCode: 'other' }],
    ['no active workspace', { activeCode: null }],
    ['no state', { state: null }],
    ['a malformed state', { state: { ...emptyOnboarding(), malformed: true } }],
    ['another intent', { state: { ...emptyOnboarding(), intent: 'agent' } }],
  ])('does not run with %s', (_name, over) => {
    expect(shouldRunLocalFirstRun({ ...base, ...over } as never)).toBe(false);
  });

  it('keeps running after a human intent was stored', () => {
    expect(shouldRunLocalFirstRun({ ...base, state: { ...emptyOnboarding(), intent: 'human' } })).toBe(true);
  });
});

describe('runLocalFirstRun', () => {
  const order: string[] = [];
  const build = (over: Partial<LocalFirstRunDeps> = {}) => {
    order.length = 0;
    let marker: FreshMarker | null = MARKER;
    const state = emptyOnboarding();
    let release!: () => void;
    const gate = new Promise<void>((resolve) => (release = resolve));
    const deps: LocalFirstRunDeps = {
      localOnly: () => true,
      mainWindow: () => true,
      marker: () => marker,
      activeCode: () => 'my-workspace',
      now: () => new Date('2026-10-12T10:05:00.000Z'),
      clearMarker: jest.fn(() => {
        order.push('clear');
        marker = null;
      }),
      state: () => state,
      setIntent: jest.fn(async (intent: string) => {
        order.push('intent');
        state.intent = intent;
      }),
      waitConsentClosed: jest.fn(async () => {
        order.push('consent-wait');
        await gate;
        order.push('consent-closed');
      }),
      startSequence: jest.fn(async () => void order.push('sequence')),
      ...over,
    };
    return { deps, release, state, marker: () => marker };
  };

  it('seeds, waits for the consent panel, starts the sequence once, then clears the marker', async () => {
    const { deps, release, marker } = build();
    const done = runLocalFirstRun(deps);
    await new Promise((r) => setTimeout(r, 0));
    expect(order).toEqual(['intent', 'consent-wait']);
    expect(deps.startSequence).not.toHaveBeenCalled();
    release();
    expect(await done).toBe('started');
    expect(order).toEqual(['intent', 'consent-wait', 'consent-closed', 'sequence', 'clear']);
    expect(deps.setIntent).toHaveBeenCalledWith('human', { seed: true, workspace: 'my-workspace' });
    expect(deps.startSequence).toHaveBeenCalledWith(LOCAL_FIRST_RUN_SEQUENCE, 'first_run');
    expect(LOCAL_FIRST_RUN_SEQUENCE).toEqual([
      'desktop.local-workspace',
      'start.first-task',
      'time.timer',
      'desktop.quick-add',
    ]);
    expect(marker()).toBeNull();
    expect(await runLocalFirstRun(deps)).toBe('none');
    expect(deps.startSequence).toHaveBeenCalledTimes(1);
  });

  it('ignores and clears a marker older than ten minutes', async () => {
    const { deps, marker } = build({ now: () => new Date('2026-10-12T10:10:01.000Z') });
    expect(await runLocalFirstRun(deps)).toBe('none');
    expect(deps.setIntent).not.toHaveBeenCalled();
    expect(marker()).toBeNull();
  });

  it('keeps the marker and does nothing while another workspace is active', async () => {
    const { deps, marker } = build({ activeCode: () => 'other' });
    expect(await runLocalFirstRun(deps)).toBe('none');
    expect(deps.setIntent).not.toHaveBeenCalled();
    expect(marker()).toEqual(MARKER);
  });

  it('seeds but starts nothing and clears the marker at once when tips are not auto', async () => {
    const { deps, state } = build({
      setIntent: jest.fn(async (intent: string) => {
        state.intent = intent;
      }),
    });
    state.tips_mode = 'hub_only';
    expect(await runLocalFirstRun(deps)).toBe('seeded');
    expect(deps.setIntent).toHaveBeenCalledTimes(1);
    expect(deps.waitConsentClosed).not.toHaveBeenCalled();
    expect(deps.startSequence).not.toHaveBeenCalled();
    expect(deps.clearMarker).toHaveBeenCalledTimes(1);
  });

  it('does not set the intent again when it is already human', async () => {
    const { deps, release, state } = build();
    state.intent = 'human';
    release();
    expect(await runLocalFirstRun(deps)).toBe('started');
    expect(deps.setIntent).not.toHaveBeenCalled();
  });

  it.each([
    ['not local', { localOnly: () => false }],
    ['a detached window', { mainWindow: () => false }],
    ['no marker', { marker: () => null }],
  ])('does nothing for %s', async (_name, over) => {
    const { deps } = build(over);
    expect(await runLocalFirstRun(deps)).toBe('none');
    expect(deps.setIntent).not.toHaveBeenCalled();
    expect(deps.startSequence).not.toHaveBeenCalled();
    expect(deps.clearMarker).not.toHaveBeenCalled();
  });

  it('keeps the marker when the sequence fails so the next launch retries', async () => {
    const { deps, release } = build({ startSequence: jest.fn().mockRejectedValue(new Error('boom')) });
    release();
    await expect(runLocalFirstRun(deps)).rejects.toThrow('boom');
    expect(deps.clearMarker).not.toHaveBeenCalled();
  });
});
