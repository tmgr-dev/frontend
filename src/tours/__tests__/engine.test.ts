import { reactive } from 'vue';
import { STARTERS_EXPECTED_KEY } from '@/onboarding/starters';
import { createTours } from '../engine';
import type { ToursHostDeps } from '../engine';
import { createTourBus } from '../events';
import { createLease } from '../lease';
import { setTourTelemetry } from '../telemetry';
import type { OnboardingApi } from '../api';

const memory = () => {
	const data = new Map<string, string>();
	return {
		getItem: (k: string) => data.get(k) ?? null,
		setItem: (k: string, v: string) => void data.set(k, v),
		removeItem: (k: string) => void data.delete(k),
	};
};

const flush = () => new Promise((r) => setImmediate(r));

const make = (over: Partial<ToursHostDeps> = {}, shared = memory()) => {
	const api: jest.Mocked<OnboardingApi> = {
		fetchState: jest.fn().mockResolvedValue({ tips_mode: 'auto', tours_generation: 1 }),
		putTour: jest.fn().mockResolvedValue({}),
		putPreferences: jest.fn().mockResolvedValue({}),
		resetTours: jest.fn().mockResolvedValue({ tours_generation: 2 }),
		setIntent: jest.fn().mockResolvedValue({}),
		fetchFacts: jest.fn().mockResolvedValue({}),
		removeStarterTasks: jest.fn().mockResolvedValue({ removed: 0 }),
	};
	const track = jest.fn();
	setTourTelemetry({ track });
	const adapter = { show: jest.fn(), refresh: jest.fn(), hide: jest.fn() };
	const nav: unknown[] = [];
	const state = { modules: reactive(new Set(['task.countdown', 'task.comments', 'board', 'workflow'])), route: 'WorkspaceTasksList', ownerId: 'a' };
	const present = new Set(['nav.sidebar', 'nav.workspace-switcher', 'nav.create-task', 'nav.user-menu', 'nav.getting-started']);
	const bus = createTourBus();
	const deps: ToursHostDeps = {
		api,
		bus,
		lease: createLease({ storage: shared, now: () => Date.now(), ownerId: state.ownerId }),
		storage: memory(),
		session: memory(),
		now: () => Date.now(),
		platform: () => 'web',
		mobile: () => false,
		account: () => 'cloud',
		isOwner: () => true,
		modulesKnown: () => true,
		isModuleEnabled: (k) => state.modules.has(k),
		currentRouteName: () => state.route,
		navigate: (r) => nav.push(r),
		isBusy: () => false,
		overlay: () => ({ adapter }),
		setTimer: () => () => {},
		resolveAnchor: async (step) => (present.has(step.anchor!) ? ({ id: step.anchor } as unknown as Element) : null),
		...over,
	};
	const engine = createTours(deps);
	return { engine, api, adapter, track, nav, state, bus, present, deps };
};

describe('engine facade', () => {
	it('start drives the overlay adapter and exposes reactive active state', async () => {
		const h = make();
		await h.engine.init();
		await h.engine.api.start('start.nav', 'hub');
		const active = h.engine.api.active.value;
		expect(active).toMatchObject({ index: 0, total: 5, waiting: false });
		expect(active?.tour.id).toBe('start.nav');
		expect(h.adapter.show).toHaveBeenCalledTimes(1);
		expect(h.adapter.show.mock.calls[0][0]).toMatchObject({ mode: 'modal' });
		h.engine.api.next();
		await flush();
		expect(h.engine.api.active.value?.index).toBe(1);
		h.engine.api.pause();
		expect(h.engine.api.active.value).toBeNull();
		expect(h.adapter.hide).toHaveBeenCalled();
		await h.engine.store.flush();
		expect(h.api.putTour).toHaveBeenLastCalledWith('start.nav', {
			status: 'paused',
			step_id: 's2',
			version: 1,
			platform: 'web',
			generation: 1,
		});
	});

	it('resume continues from the saved step', async () => {
		const h = make();
		await h.engine.init();
		await h.engine.api.start('start.nav', 'hub');
		h.engine.api.next();
		await flush();
		h.engine.api.pause();
		await h.engine.api.resume('start.nav');
		expect(h.engine.api.active.value?.step.id).toBe('s2');
	});

	it('the lease blocks a second tab from starting a tour', async () => {
		const shared = memory();
		const first = make({}, shared);
		const second = make({ lease: createLease({ storage: shared, now: () => Date.now(), ownerId: 'b' }) }, shared);
		await first.engine.api.start('start.nav', 'hub');
		await second.engine.api.start('start.nav', 'hub');
		expect(first.engine.api.active.value).not.toBeNull();
		expect(second.engine.api.active.value).toBeNull();
		first.engine.api.pause();
		await second.engine.api.start('start.nav', 'hub');
		expect(second.engine.api.active.value).not.toBeNull();
	});

	it('hubItems reflect module, platform and role availability reactively', async () => {
		const h = make();
		await h.engine.init();
		const find = (id: string) => h.engine.api.hubItems.value.find((i) => i.tour.id === id)!;
		expect(find('time.timer').available).toBe(true);
		h.state.modules.delete('task.countdown');
		expect(find('time.timer')).toMatchObject({ available: false, unavailableReason: 'module_off' });
		h.state.modules.delete('board');
		expect(find('tasks.board-move').unavailableReason).toBe('module_off');
	});

	it('hubItems carry saved state and do not judge unknown modules', async () => {
		const h = make({ modulesKnown: () => false });
		h.api.fetchState.mockResolvedValue({ tours: { 'start.nav': { status: 'completed', step_id: null, version: 1, generation: 0 } } });
		await h.engine.init();
		const items = h.engine.api.hubItems.value;
		expect(items.find((i) => i.tour.id === 'start.nav')?.state?.status).toBe('completed');
		expect(items.filter((i) => i.tour.group !== 'desktop').every((i) => i.available)).toBe(true);
	});

	it('refuses to start a tour unavailable on this platform', async () => {
		const h = make({ platform: () => 'ios' });
		await h.engine.api.start('start.nav', 'hub');
		expect(h.engine.api.active.value).toBeNull();
	});

	it('module switched off mid-tour pauses it', async () => {
		const h = make();
		await h.engine.api.start('time.timer', 'hub');
		h.present.add('task-form.timer');
		h.state.modules.delete('task.countdown');
		h.engine.modulesChanged();
		expect(h.engine.api.active.value).toBeNull();
		expect(h.engine.store.tourState('time.timer')?.status).toBe('paused');
	});

	it('navigating away from a routed step pauses and shows the resume pill', async () => {
		let t = 1_000_000;
		const h = make({ now: () => (t += 10_000) });
		h.present.add('hub.catalog');
		await h.engine.api.start('start.hub', 'hub');
		expect(h.engine.api.active.value?.step.id).toBe('s2');
		h.engine.routeEntered('WorkspaceBoard');
		expect(h.engine.api.active.value).toBeNull();
		expect(h.engine.api.resumePill.value).toEqual({ tourId: 'start.hub', title: 'Your Getting started page' });
		await h.engine.api.resume('start.hub');
		expect(h.engine.api.resumePill.value).toBeNull();
		expect(h.engine.api.active.value?.step.id).toBe('s2');
	});

	it('an unavailable step is shown centered without blocking the page', async () => {
		const h = make();
		await h.engine.init();
		await h.engine.api.start('time.timer', 'hub');
		expect(h.engine.api.active.value).toMatchObject({ unavailable: true, canTakeMe: true });
		expect(h.adapter.show.mock.calls[0][0]).toMatchObject({ mode: 'centered', blocking: false });
		await h.engine.api.start('start.nav', 'hub');
		expect(h.adapter.show.mock.calls[h.adapter.show.mock.calls.length - 1][0]).toMatchObject({ mode: 'modal', blocking: true });
	});

	it('setTipsMode persists, tracks and rejects on failure', async () => {
		const h = make();
		await h.engine.init();
		await h.engine.api.setTipsMode('off');
		expect(h.api.putPreferences).toHaveBeenCalledWith({ tips_mode: 'off' });
		expect(h.track).toHaveBeenCalledWith('tips_mode_changed', { mode: 'off' });
		expect(h.engine.api.onboarding.value?.tips_mode).toBe('off');
	});

	it('setHubHidden persists and updates onboarding state', async () => {
		const h = make();
		await h.engine.init();
		await h.engine.api.setHubHidden(true);
		expect(h.api.putPreferences).toHaveBeenCalledWith({ hub_hidden: true });
		expect(h.engine.api.onboarding.value?.hub_hidden).toBe(true);
	});

	it('resetAll abandons the active tour, clears state and bumps the generation', async () => {
		const h = make();
		await h.engine.init();
		await h.engine.api.start('start.nav', 'hub');
		await h.engine.api.resetAll();
		expect(h.engine.api.active.value).toBeNull();
		expect(h.engine.api.onboarding.value?.tours).toEqual({});
		expect(h.engine.api.onboarding.value?.tours_generation).toBe(2);
	});

	it('first_visit trigger starts G2 on the list for a fresh account and respects tips_mode', async () => {
		const timers: Array<() => void> = [];
		const h = make({ setTimer: (fn) => { timers.push(fn); return () => {}; } });
		await h.engine.init();
		h.present.add('nav.create-task');
		h.engine.routeEntered('WorkspaceTasksList');
		timers.splice(0).forEach((f) => f());
		await flush();
		expect(h.engine.api.active.value?.tour.id).toBe('start.first-task');
		expect(h.track).toHaveBeenCalledWith('tour_offered', { tour: 'start.first-task', trigger: 'first_visit' });
	});

	it.each([
		[{ tips_mode: 'hub_only', tours_generation: 1 }],
		[{ tips_mode: 'off', tours_generation: 1 }],
		[{ tips_mode: 'auto', tours_generation: 1, facts: { has_task: true } }],
	])('first_visit does not auto-start G2 for %j', async (state: object) => {
		const timers: Array<() => void> = [];
		const h = make({ setTimer: (fn) => { timers.push(fn); return () => {}; } });
		h.api.fetchState.mockResolvedValue(state);
		await h.engine.init();
		h.present.add('nav.create-task');
		h.engine.routeEntered('WorkspaceTasksList');
		timers.splice(0).forEach((f) => f());
		await flush();
		expect(h.engine.api.active.value).toBeNull();
	});

	it('first_visit starts G2 when has_task is false', async () => {
		const timers: Array<() => void> = [];
		const h = make({ setTimer: (fn) => { timers.push(fn); return () => {}; } });
		h.api.fetchState.mockResolvedValue({ tips_mode: 'auto', tours_generation: 1, facts: { has_task: false } });
		await h.engine.init();
		h.present.add('nav.create-task');
		h.engine.routeEntered('WorkspaceTasksList');
		timers.splice(0).forEach((f) => f());
		await flush();
		expect(h.engine.api.active.value?.tour.id).toBe('start.first-task');
	});

	it('wait step advances from a bus event through the facade', async () => {
		const h = make();
		await h.engine.init();
		h.present.add('task-form.title');
		await h.engine.api.start('start.first-task', 'hub');
		expect(h.engine.api.active.value).toMatchObject({ waiting: true });
		h.bus.emit('task.create_opened');
		await flush();
		expect(h.engine.api.active.value?.step.id).toBe('s2');
	});

	it('re-runs first_visit for the current route when the state loads after the route was entered', async () => {
		const timers: Array<() => void> = [];
		const h = make({ setTimer: (fn) => { timers.push(fn); return () => {}; } });
		h.present.add('nav.create-task');
		h.engine.routeEntered('WorkspaceTasksList');
		timers.splice(0).forEach((f) => f());
		await flush();
		expect(h.engine.api.active.value).toBeNull();
		await h.engine.init();
		timers.splice(0).forEach((f) => f());
		await flush();
		expect(h.engine.api.active.value?.tour.id).toBe('start.first-task');
	});

	it('signedOut abandons the active tour and drops the previous account state', async () => {
		const h = make();
		await h.engine.init();
		await h.engine.api.start('start.nav', 'hub');
		expect(h.engine.api.active.value).not.toBeNull();
		h.engine.signedOut();
		expect(h.engine.api.active.value).toBeNull();
		expect(h.engine.api.onboarding.value).toBeNull();
		h.api.fetchState.mockResolvedValue({ tips_mode: 'auto', tours_generation: 0 });
		await h.engine.init();
		expect(h.engine.api.onboarding.value?.tours_generation).toBe(0);
	});

	it('hostReady follows the overlay registration', () => {
		let registered = false;
		const h = make({ overlay: () => (registered ? { adapter: { show() {}, refresh() {}, hide() {} } } : null) });
		expect(h.engine.api.hostReady.value).toBe(false);
		registered = true;
		h.engine.rerender();
		expect(h.engine.api.hostReady.value).toBe(true);
	});
});

describe('startSequence', () => {
	const walk = async (h: ReturnType<typeof make>, times: number) => {
		for (let i = 0; i < times; i++) {
			h.engine.api.next();
			await flush();
		}
	};

	it('chains the tours on completion and marks the session as used', async () => {
		const session = memory();
		const h = make({ session });
		await h.engine.init();
		await h.engine.api.startSequence(['start.nav', 'start.first-task'], 'first_run');
		expect(h.engine.api.active.value?.tour.id).toBe('start.nav');
		expect(session.getItem('tmgr:tours:auto')).toBe('true');
		await walk(h, 5);
		expect(h.engine.api.active.value?.tour.id).toBe('start.first-task');
		expect(h.track).toHaveBeenCalledWith('tour_started', { tour: 'start.first-task', trigger: 'first_run' });
	});

	it('does nothing unless tips mode is auto', async () => {
		const session = memory();
		const h = make({ session });
		h.api.fetchState.mockResolvedValue({ tips_mode: 'hub_only', tours_generation: 1 });
		await h.engine.init();
		await h.engine.api.startSequence(['start.nav'], 'first_run');
		expect(h.engine.api.active.value).toBeNull();
		expect(session.getItem('tmgr:tours:auto')).toBeNull();
	});

	it('skipping a tour ends the sequence', async () => {
		const h = make();
		await h.engine.init();
		await h.engine.api.startSequence(['start.nav', 'start.first-task'], 'first_run');
		h.engine.api.skipTour();
		await flush();
		expect(h.engine.api.active.value).toBeNull();
	});

	it('navigating away ends the sequence', async () => {
		const h = make();
		await h.engine.init();
		h.state.route = 'WorkspaceGettingStarted';
		['hub.checklist', 'hub.catalog', 'hub.tips-toggle'].forEach((a) => h.present.add(a));
		await h.engine.api.startSequence(['start.hub', 'start.first-task'], 'first_run');
		h.state.route = 'Board';
		h.engine.routeEntered('Board');
		await flush();
		expect(h.engine.api.active.value).toBeNull();
		expect(h.engine.api.resumePill.value?.tourId).toBe('start.hub');
	});

	it('pausing ends the sequence', async () => {
		const h = make();
		await h.engine.init();
		await h.engine.api.startSequence(['start.nav', 'start.first-task'], 'first_run');
		h.engine.api.pause();
		await flush();
		expect(h.engine.api.active.value).toBeNull();
	});

	it('skips an unavailable tour in the chain', async () => {
		const h = make();
		await h.engine.init();
		h.state.modules.delete('task.countdown');
		await h.engine.api.startSequence(['start.nav', 'time.timer', 'start.first-task'], 'first_run');
		await walk(h, 5);
		expect(h.engine.api.active.value?.tour.id).toBe('start.first-task');
	});

	it('starts nothing and frees the session when no tour is available', async () => {
		const session = memory();
		const h = make({ session });
		await h.engine.init();
		h.state.modules.delete('task.countdown');
		await h.engine.api.startSequence(['time.timer'], 'first_run');
		expect(h.engine.api.active.value).toBeNull();
		expect(session.getItem('tmgr:tours:auto')).toBe('false');
	});

	it('signing out drops the pending chain', async () => {
		const h = make();
		await h.engine.init();
		await h.engine.api.startSequence(['start.nav', 'start.first-task'], 'first_run');
		h.engine.signedOut();
		await flush();
		expect(h.engine.api.active.value).toBeNull();
	});
});

describe('start.connect-agent', () => {
	const connect = async (over: Partial<ToursHostDeps> = {}, state: object = {}) => {
		const h = make(over);
		h.state.route = 'WelcomeSetupAgent';
		h.state.modules.add('mcp');
		h.api.fetchState.mockResolvedValue({ tips_mode: 'auto', tours_generation: 1, ...state });
		['connect.client-tabs', 'connect.command', 'connect.status', 'connect.prompt'].forEach((a) => h.present.add(a));
		await h.engine.init();
		await h.engine.api.start('start.connect-agent', 'first_run');
		return h;
	};

	it('does not complete when the first MCP call lands, so the prompt step still shows', async () => {
		const h = await connect();
		h.engine.api.next();
		await flush();
		h.bus.emit('connect.command_copied');
		await flush();
		expect(h.engine.api.active.value?.step.id).toBe('s3');
		h.bus.emit('agent.first_mcp_call');
		await flush();
		expect(h.engine.api.active.value?.step.id).toBe('s4');
		await h.engine.store.flush();
		expect(h.api.putTour).not.toHaveBeenCalledWith('start.connect-agent', expect.objectContaining({ status: 'completed' }));
		h.engine.api.next();
		await flush();
		expect(h.engine.api.active.value).toBeNull();
		await h.engine.store.flush();
		expect(h.api.putTour).toHaveBeenLastCalledWith('start.connect-agent', expect.objectContaining({ status: 'completed' }));
	});

	it('skips the copy step and shows the connected step as already done when the fact is set', async () => {
		const h = await connect({}, { facts: { mcp_first_call_at: '2026-10-12T10:00:00Z' } });
		h.engine.api.next();
		await flush();
		expect(h.engine.api.active.value).toMatchObject({ alreadyDone: true });
		expect(h.engine.api.active.value?.step.id).toBe('s3');
	});

	it('moves off the copy step when the screen reports the agent connected', async () => {
		const h = await connect();
		h.engine.api.next();
		await flush();
		expect(h.engine.api.active.value?.step.id).toBe('s2');
		h.engine.api.patchFacts({ mcp_first_call_at: '2026-10-12T10:00:00Z' });
		await flush();
		expect(h.engine.api.active.value?.step.id).toBe('s3');
		expect(h.engine.api.active.value).toMatchObject({ alreadyDone: true });
	});

	it('is unavailable when the mcp module is off or the account is local', async () => {
		const h = make();
		await h.engine.init();
		const item = () => h.engine.api.hubItems.value.find((i) => i.tour.id === 'start.connect-agent')!;
		expect(item().available).toBe(false);
		h.state.modules.add('mcp');
		expect(item().available).toBe(true);
		const local = make({ account: () => 'local' });
		local.state.modules.add('mcp');
		await local.engine.init();
		expect(local.engine.api.hubItems.value.find((i) => i.tour.id === 'start.connect-agent')?.unavailableReason).toBe('account');
	});
});

describe('setIntent and checklist', () => {
	it('applies the intent optimistically and passes seed through', async () => {
		const h = make();
		await h.engine.init();
		let seenDuringCall: string | null = null;
		h.api.setIntent.mockImplementation(async () => {
			seenDuringCall = h.engine.api.onboarding.value?.intent ?? null;
			return { intent: 'human', starter_tasks_created: 3, invited: false };
		});
		await h.engine.api.setIntent('human');
		expect(seenDuringCall).toBe('human');
		expect(h.api.setIntent).toHaveBeenCalledWith('human', undefined);
		expect(h.engine.api.onboarding.value).toMatchObject({ intent: 'human', invited: false });
		expect(h.api.removeStarterTasks).not.toHaveBeenCalled();
	});

	it('deletes the starter tasks when seed:false still created some', async () => {
		const h = make();
		await h.engine.init();
		h.api.setIntent.mockResolvedValue({ intent: 'human', starter_tasks_created: 3 });
		await h.engine.api.setIntent('human', { seed: false });
		expect(h.api.setIntent).toHaveBeenCalledWith('human', { seed: false });
		expect(h.api.removeStarterTasks).toHaveBeenCalledTimes(1);
	});

	it('does not delete when seed:false created none, and tolerates a failing delete', async () => {
		const h = make();
		await h.engine.init();
		h.api.setIntent.mockResolvedValue({ intent: 'human', starter_tasks_created: 0 });
		await h.engine.api.setIntent('human', { seed: false });
		expect(h.api.removeStarterTasks).not.toHaveBeenCalled();
		h.api.setIntent.mockResolvedValue({ intent: 'human', starter_tasks_created: 2 });
		h.api.removeStarterTasks.mockRejectedValue(new Error('net'));
		jest.spyOn(console, 'error').mockImplementation(() => {});
		await expect(h.engine.api.setIntent('human', { seed: false })).resolves.toBeUndefined();
	});

	it('records that starter tasks exist so the hub can offer their removal', async () => {
		const h = make();
		await h.engine.init();
		h.api.setIntent.mockResolvedValue({ intent: 'human', starter_tasks_created: 3 });
		await h.engine.api.setIntent('human');
		expect(h.deps.storage.getItem(STARTERS_EXPECTED_KEY)).toBe('3');
	});

	it('records nothing when none were created or the empty workspace removed them', async () => {
		const h = make();
		await h.engine.init();
		h.api.setIntent.mockResolvedValue({ intent: 'human', starter_tasks_created: 3 });
		await h.engine.api.setIntent('human', { seed: false });
		expect(h.deps.storage.getItem(STARTERS_EXPECTED_KEY)).toBeFalsy();
		h.api.setIntent.mockResolvedValue({ intent: 'human', starter_tasks_created: 0 });
		await h.engine.api.setIntent('human');
		expect(h.deps.storage.getItem(STARTERS_EXPECTED_KEY)).toBeFalsy();
	});

	it('rolls the intent back and rethrows when the request fails', async () => {
		const h = make();
		await h.engine.init();
		h.api.setIntent.mockRejectedValue(new Error('net'));
		await expect(h.engine.api.setIntent('agent')).rejects.toThrow('net');
		expect(h.engine.api.onboarding.value?.intent).toBeNull();
	});

	it.each([['skipped', 'true'], ['agent', 'true'], ['human', null]])(
		'intent %s sets the session auto flag to %s',
		async (intent, expected) => {
			const session = memory();
			const h = make({ session });
			await h.engine.init();
			await h.engine.api.setIntent(intent);
			expect(session.getItem('tmgr:tours:auto')).toBe(expected);
		},
	);

	it('setChecklistHidden persists the preference', async () => {
		const h = make();
		await h.engine.init();
		await h.engine.api.setChecklistHidden(true);
		expect(h.api.putPreferences).toHaveBeenCalledWith({ checklist_hidden: true });
		expect(h.engine.api.onboarding.value?.checklist_hidden).toBe(true);
	});
});

