import { createTourBus } from '../events';
import { durBucket, TourRunner } from '../runner';
import type { RunnerDeps, RunnerView } from '../runner';
import { setTourTelemetry } from '../telemetry';
import type { StepDef, TourDef, TourState } from '../types';

const step = (id: string, extra: Partial<StepDef> = {}): StepDef => ({
	id,
	type: 'spot',
	anchor: `a.${id}`,
	title: id,
	body: id,
	...extra,
});

const tour = (extra: Partial<TourDef> = {}): TourDef => ({
	id: 't.one',
	version: 1,
	since: '1',
	group: 'start',
	title: 'T',
	minutes: 1,
	triggers: [{ kind: 'hub' }],
	steps: [step('s1'), step('s2', { type: 'wait', waitFor: { event: 'task.created' } }), step('s3')],
	...extra,
});

const setup = (tours: TourDef[] = [tour()], overrides: Partial<RunnerDeps> = {}) => {
	const bus = createTourBus();
	const present = new Set<string>();
	const views: Array<RunnerView | null> = [];
	const saves: any[] = [];
	const nav: any[] = [];
	const timers: Array<() => void> = [];
	const track = jest.fn();
	setTourTelemetry({ track });
	let now = 1_000_000;
	let route: string | null = 'WorkspaceTasksList';
	let modulesOn = new Set<string>(['board', 'workflow']);
	let facts: Record<string, unknown> = {};
	let saved: Record<string, TourState> = {};
	const deps: RunnerDeps = {
		getTour: (id) => tours.find((t) => t.id === id),
		stateOf: (id) => saved[id] ?? null,
		now: () => now,
		mobile: () => false,
		modulesKnown: () => true,
		isModuleEnabled: (k) => modulesOn.has(k),
		currentRouteName: () => route,
		navigate: (r) => nav.push(r),
		resolveAnchor: async (s) => (present.has(s.anchor!) || present.has(s.mobileAnchor ?? '#') ? ({ id: s.anchor } as unknown as Element) : null),
		facts: () => facts,
		save: (id, input) => {
			saves.push([id, input]);
			saved[id] = { ...input, generation: 0 };
		},
		bus,
		render: (v) => views.push(v),
		setTimer: (fn) => {
			timers.push(fn);
			return () => {};
		},
		...overrides,
	};
	const runner = new TourRunner(deps);
	const last = () => views[views.length - 1];
	const calls = (name: string) => track.mock.calls.filter((c) => c[0] === name).map((c) => c[1]);
	return {
		runner,
		bus,
		present,
		views,
		saves,
		nav,
		timers,
		track,
		last,
		calls,
		setNow: (n: number) => (now = n),
		setRoute: (r: string | null) => (route = r),
		setModules: (m: string[]) => (modulesOn = new Set(m)),
		setFacts: (f: Record<string, unknown>) => (facts = f),
		setSaved: (s: Record<string, TourState>) => (saved = s),
		tick: () => timers.splice(0).forEach((f) => f()),
	};
};

const flush = () => new Promise((r) => setImmediate(r));

describe('durBucket', () => {
	it.each([
		[59_999, 'lt_1m'],
		[60_000, 'm1_3'],
		[179_999, 'm1_3'],
		[180_000, 'm3_10'],
		[599_999, 'm3_10'],
		[600_000, 'gt_10m'],
	])('%i -> %s', (ms, bucket) => expect(durBucket(ms)).toBe(bucket));
});

describe('TourRunner', () => {
	it('starts, shows step 1 modal with anchor, persists and tracks', async () => {
		const h = setup();
		h.present.add('a.s1');
		expect(await h.runner.start('t.one', 'hub')).toBe(true);
		expect(h.last()).toMatchObject({ index: 0, total: 3, mode: 'modal', waiting: false });
		expect(h.last()?.step.id).toBe('s1');
		expect(h.saves).toEqual([['t.one', { status: 'in_progress', step_id: 's1', version: 1 }]]);
		expect(h.calls('tour_started')).toEqual([{ tour: 't.one', trigger: 'hub' }]);
		expect(h.calls('tour_step_viewed')).toEqual([{ tour: 't.one', step: 's1', i: 1 }]);
	});

	it('returns false for an unknown tour', async () => {
		expect(await setup().runner.start('nope', 'hub')).toBe(false);
	});

	it('next advances, tracks step_completed via next, and a wait step is passive and ignores next', async () => {
		const h = setup();
		['a.s1', 'a.s2', 'a.s3'].forEach((a) => h.present.add(a));
		await h.runner.start('t.one', 'hub');
		await h.runner.next();
		expect(h.last()).toMatchObject({ index: 1, mode: 'passive', waiting: true });
		expect(h.calls('tour_step_completed')).toEqual([{ tour: 't.one', step: 's1', i: 1, via: 'next' }]);
		await h.runner.next();
		expect(h.last()?.index).toBe(1);
	});

	it('wait step completes on the bus event (via action), where is respected', async () => {
		const t = tour({
			steps: [step('w', { type: 'wait', waitFor: { event: 'task.status_changed', where: { statusType: 'active' } } }), step('after')],
		});
		const h = setup([t]);
		h.present.add('a.w');
		h.present.add('a.after');
		await h.runner.start('t.one', 'hub');
		h.bus.emit('task.status_changed', { statusType: 'hidden' });
		await flush();
		expect(h.last()?.step.id).toBe('w');
		h.bus.emit('task.status_changed', { statusType: 'active' });
		await flush();
		expect(h.last()?.step.id).toBe('after');
		expect(h.calls('tour_step_completed')[0]).toMatchObject({ via: 'action' });
	});

	it('finishes with completed, dur bucket and a final persisted state', async () => {
		const h = setup([tour({ steps: [step('only')] })]);
		h.present.add('a.only');
		await h.runner.start('t.one', 'hub');
		h.setNow(1_000_000 + 90_000);
		await h.runner.next();
		expect(h.last()).toBeNull();
		expect(h.calls('tour_completed')).toEqual([{ tour: 't.one', dur: 'm1_3' }]);
		expect(h.saves.pop()).toEqual(['t.one', { status: 'completed', step_id: null, version: 1 }]);
		expect(h.runner.activeTourId).toBeNull();
	});

	it('completeOn marks completed once while remaining steps keep showing', async () => {
		const t = tour({
			completeOn: 'task.created',
			steps: [step('w', { type: 'wait', waitFor: { event: 'task.created' } }), step('tail')],
		});
		const h = setup([t]);
		h.present.add('a.w');
		h.present.add('a.tail');
		await h.runner.start('t.one', 'hub');
		h.bus.emit('task.created', { taskId: 5 });
		await flush();
		expect(h.last()?.step.id).toBe('tail');
		expect(h.calls('tour_completed')).toHaveLength(1);
		await h.runner.next();
		expect(h.last()).toBeNull();
		expect(h.calls('tour_completed')).toHaveLength(1);
		expect(h.saves.filter((s) => s[1].status === 'completed')).toHaveLength(1);
	});

	it('skipStep moves on without step_completed and records the skip', async () => {
		const onSkip = jest.fn();
		const h = setup([tour()], { onSkip });
		['a.s1', 'a.s2'].forEach((a) => h.present.add(a));
		await h.runner.start('t.one', 'hub');
		await h.runner.skipStep();
		expect(h.last()?.step.id).toBe('s2');
		expect(h.calls('tour_step_skipped')).toEqual([{ tour: 't.one', step: 's1', i: 1 }]);
		expect(h.calls('tour_step_completed')).toEqual([]);
		expect(onSkip).toHaveBeenCalledWith('t.one');
	});

	it('skipTour persists skipped, tracks and closes', async () => {
		const onFinished = jest.fn();
		const h = setup([tour()], { onFinished });
		h.present.add('a.s1');
		await h.runner.start('t.one', 'hub');
		h.runner.skipTour();
		expect(h.last()).toBeNull();
		expect(h.calls('tour_skipped')).toEqual([{ tour: 't.one', i: 1 }]);
		expect(h.saves.pop()[1].status).toBe('skipped');
		expect(onFinished).toHaveBeenCalledWith('t.one', 'skipped');
	});

	it('pause saves the step id; resume continues at that step id', async () => {
		const h = setup();
		['a.s1', 'a.s2', 'a.s3'].forEach((a) => h.present.add(a));
		await h.runner.start('t.one', 'hub');
		await h.runner.next();
		h.runner.pause();
		expect(h.last()).toBeNull();
		expect(h.saves.pop()).toEqual(['t.one', { status: 'paused', step_id: 's2', version: 1 }]);
		expect(h.calls('tour_paused')).toEqual([{ tour: 't.one', i: 2 }]);
		await h.runner.resume('t.one');
		expect(h.last()?.step.id).toBe('s2');
		expect(h.calls('tour_started')[1]).toEqual({ tour: 't.one', trigger: 'resume' });
	});

	it('resume with an unknown step id restarts; completed tours restart too', async () => {
		const h = setup();
		['a.s1', 'a.s2', 'a.s3'].forEach((a) => h.present.add(a));
		h.setSaved({ 't.one': { status: 'paused', step_id: 'gone', version: 1, generation: 0 } });
		await h.runner.resume('t.one');
		expect(h.last()?.step.id).toBe('s1');
		h.runner.pause();
		h.setSaved({ 't.one': { status: 'completed', step_id: null, version: 1, generation: 0 } });
		await h.runner.resume('t.one');
		expect(h.last()?.step.id).toBe('s1');
	});

	it('skips an optional step with a missing anchor', async () => {
		const t = tour({ steps: [step('o', { optional: true }), step('real')] });
		const h = setup([t]);
		h.present.add('a.real');
		await h.runner.start('t.one', 'hub');
		expect(h.last()?.step.id).toBe('real');
		expect(h.calls('tour_step_unavailable')).toEqual([{ tour: 't.one', step: 'o' }]);
	});

	it('completes when the only remaining optional steps are missing', async () => {
		const h = setup([tour({ steps: [step('o', { optional: true })] })]);
		await h.runner.start('t.one', 'hub');
		expect(h.last()).toBeNull();
		expect(h.calls('tour_completed')).toHaveLength(1);
	});

	it('shows a centered unavailable state for a required missing anchor, with Take me there', async () => {
		const t = tour({ steps: [step('r', { takeMeRoute: { name: 'WorkspaceTasksList', params: 'currentWorkspace' } }), step('n')] });
		const h = setup([t]);
		await h.runner.start('t.one', 'hub');
		expect(h.last()).toMatchObject({ mode: 'centered', unavailable: true, canTakeMe: true, waiting: false });
		await h.runner.next();
		expect(h.last()?.step.id).toBe('r');
		h.present.add('a.r');
		await h.runner.takeMeThere();
		expect(h.nav).toEqual([{ name: 'WorkspaceTasksList', params: 'currentWorkspace' }]);
		expect(h.last()).toMatchObject({ mode: 'modal', unavailable: false });
	});

	it('re-enters an unavailable step as soon as its anchor shows up', async () => {
		let appear: (el: Element) => void = () => {};
		const waitForAnchor = jest.fn(
			() => new Promise<Element | null>((resolve) => (appear = resolve)),
		);
		const t = tour({ steps: [step('r', { type: 'wait', waitFor: { event: 'timer.started' } }), step('n')] });
		const h = setup([t], { waitForAnchor });
		await h.runner.start('t.one', 'hub');
		expect(h.last()).toMatchObject({ unavailable: true, mode: 'centered' });
		h.present.add('a.r');
		appear({} as Element);
		await flush();
		expect(h.last()).toMatchObject({ unavailable: false, mode: 'passive', waiting: true });
	});

	it('stops watching for the anchor once the tour is paused', async () => {
		let signal: AbortSignal | undefined;
		const waitForAnchor = jest.fn((_s: unknown, o: { signal: AbortSignal }) => {
			signal = o.signal;
			return new Promise<Element | null>(() => {});
		});
		const h = setup([tour({ steps: [step('r'), step('n')] })], { waitForAnchor });
		await h.runner.start('t.one', 'hub');
		h.runner.pause();
		expect(signal?.aborted).toBe(true);
	});

	it('canTakeMe is false without a route; skipStep gets past it', async () => {
		const t = tour({ steps: [step('r'), step('n')] });
		const h = setup([t]);
		h.present.add('a.n');
		await h.runner.start('t.one', 'hub');
		expect(h.last()).toMatchObject({ unavailable: true, canTakeMe: false });
		await h.runner.skipStep();
		expect(h.last()?.step.id).toBe('n');
	});

	it('navigates to the step route when elsewhere and does not pause for its own navigation', async () => {
		const t = tour({ steps: [step('b', { route: { name: 'WorkspaceBoard', params: 'currentWorkspace' } })] });
		const h = setup([t]);
		h.present.add('a.b');
		await h.runner.start('t.one', 'hub');
		expect(h.nav).toEqual([{ name: 'WorkspaceBoard', params: 'currentWorkspace' }]);
		h.setRoute('WorkspaceBoard');
		h.runner.routeChanged('WorkspaceBoard');
		expect(h.last()).not.toBeNull();
		h.runner.routeChanged('Other');
		expect(h.last()).not.toBeNull();
		h.setNow(1_000_000 + 5000);
		h.runner.routeChanged('Other');
		expect(h.last()).toBeNull();
		expect(h.saves.pop()[1].status).toBe('paused');
	});

	it('Already done advances via facts after a short delay', async () => {
		const t = tour({ steps: [step('w', { type: 'wait', waitFor: { event: 'timer.started' }, alreadyDone: 'has_timer_minute' }), step('after')] });
		const h = setup([t]);
		h.present.add('a.w');
		h.present.add('a.after');
		h.setFacts({ has_timer_minute: true });
		await h.runner.start('t.one', 'hub');
		expect(h.last()).toMatchObject({ alreadyDone: true, waiting: false });
		h.tick();
		await flush();
		expect(h.last()?.step.id).toBe('after');
		expect(h.calls('tour_step_completed')[0]).toMatchObject({ via: 'action' });
	});

	it('Already done also advances via a previously seen event', async () => {
		const t = tour({ steps: [step('w', { type: 'wait', waitFor: { event: 'comment.created' }, alreadyDone: 'comment.created' }), step('after')] });
		const h = setup([t]);
		h.present.add('a.w');
		h.present.add('a.after');
		h.bus.emit('comment.created', { taskId: 1 });
		await h.runner.start('t.one', 'hub');
		expect(h.last()?.alreadyDone).toBe(true);
	});

	it('pauses when a required module is switched off mid-tour, and refuses to start without it', async () => {
		const t = tour({ requires: { modules: ['board'] } });
		const onFinished = jest.fn();
		const h = setup([t], { onFinished });
		h.present.add('a.s1');
		await h.runner.start('t.one', 'hub');
		h.setModules(['workflow']);
		h.runner.recheck();
		expect(h.last()).toBeNull();
		expect(h.saves.pop()[1].status).toBe('paused');
		expect(onFinished).toHaveBeenCalledWith('t.one', 'module_off');
		expect(await h.runner.start('t.one', 'hub')).toBe(false);
	});

	it('does not judge modules before they are known', async () => {
		const h = setup([tour({ requires: { modules: ['board'] } })], { modulesKnown: () => false });
		h.present.add('a.s1');
		h.setModules([]);
		expect(await h.runner.start('t.one', 'hub')).toBe(true);
		h.runner.recheck();
		expect(h.last()).not.toBeNull();
	});

	it('skips steps whose requiresModules are off', async () => {
		const t = tour({ steps: [step('a', { requiresModules: ['task.assignees'] }), step('b')] });
		const h = setup([t]);
		h.present.add('a.a');
		h.present.add('a.b');
		await h.runner.start('t.one', 'hub');
		expect(h.last()?.step.id).toBe('b');
	});

	it('timer.stopped needs >= 60s since timer.started on the same task', async () => {
		const t = tour({
			completeOn: 'timer.stopped',
			steps: [
				step('start', { type: 'wait', waitFor: { event: 'timer.started' } }),
				step('stop', { type: 'wait', waitFor: { event: 'timer.stopped' } }),
				step('end', { optional: true }),
			],
		});
		const h = setup([t]);
		h.present.add('a.start');
		h.present.add('a.stop');
		await h.runner.start('t.one', 'hub');
		h.bus.emit('timer.started', { taskId: 9 });
		await flush();
		expect(h.last()?.step.id).toBe('stop');
		h.setNow(1_000_000 + 30_000);
		h.bus.emit('timer.stopped', { taskId: 9 });
		await flush();
		expect(h.last()).toMatchObject({ hint: 'timer_too_short', waiting: true });
		expect(h.calls('tour_completed')).toHaveLength(0);
		h.bus.emit('timer.started', { taskId: 9 });
		h.setNow(1_000_000 + 30_000 + 61_000);
		h.bus.emit('timer.stopped', { taskId: 9 });
		await flush();
		expect(h.calls('tour_completed')).toHaveLength(1);
		expect(h.last()).toBeNull();
	});

	it('anchorScope eventTask receives the id of the last task.created', async () => {
		const resolveAnchor = jest.fn().mockResolvedValue({} as Element);
		const t = tour({ steps: [step('w', { type: 'wait', waitFor: { event: 'task.created' } }), step('row', { anchorScope: 'eventTask' })] });
		const h = setup([t], { resolveAnchor });
		await h.runner.start('t.one', 'hub');
		h.bus.emit('task.created', { taskId: 42 });
		await flush();
		expect(resolveAnchor.mock.calls[1][1]).toMatchObject({ eventTaskId: 42 });
	});

	it('uses the mobile anchor and reports mobile in the view', async () => {
		const t = tour({ steps: [step('m', { mobileAnchor: 'nav.menu-button' })] });
		const h = setup([t], { mobile: () => true });
		h.present.add('nav.menu-button');
		await h.runner.start('t.one', 'hub');
		expect(h.last()).toMatchObject({ mobile: true, unavailable: false });
	});

	it('a step without an anchor is a centered card', async () => {
		const t = tour({ steps: [step('m', { type: 'modal', anchor: undefined })] });
		const h = setup([t]);
		await h.runner.start('t.one', 'hub');
		expect(h.last()).toMatchObject({ mode: 'centered', unavailable: false });
	});

	it('starting another tour pauses the active one (one tour at a time)', async () => {
		const other = tour({ id: 't.two' });
		const h = setup([tour(), other]);
		h.present.add('a.s1');
		await h.runner.start('t.one', 'hub');
		await h.runner.start('t.two', 'hub');
		expect(h.runner.activeTourId).toBe('t.two');
		expect(h.saves.find((s) => s[0] === 't.one' && s[1].status === 'paused')).toBeTruthy();
	});

	it('ignores a stale anchor resolution after the step changed', async () => {
		let release: (el: Element | null) => void = () => {};
		const slow = new Promise<Element | null>((r) => (release = r));
		const resolveAnchor = jest.fn().mockReturnValueOnce(slow).mockResolvedValue({} as Element);
		const h = setup([tour()], { resolveAnchor });
		const first = h.runner.start('t.one', 'hub');
		await flush();
		h.runner.pause();
		release({} as Element);
		await first;
		expect(h.last()).toBeNull();
	});

	it('respects eligible()', async () => {
		const h = setup([tour()], { eligible: () => false });
		expect(await h.runner.start('t.one', 'hub')).toBe(false);
	});
});
