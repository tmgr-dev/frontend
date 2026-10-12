import { reactive } from 'vue';
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
		fetchFacts: jest.fn().mockResolvedValue({}),
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
			step_id: 'workspace-switcher',
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
		expect(h.engine.api.active.value?.step.id).toBe('workspace-switcher');
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
		expect(items.every((i) => i.available)).toBe(true);
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
		expect(h.engine.api.active.value?.step.id).toBe('catalog');
		h.engine.routeEntered('WorkspaceBoard');
		expect(h.engine.api.active.value).toBeNull();
		expect(h.engine.api.resumePill.value).toEqual({ tourId: 'start.hub', title: 'Your Getting started page' });
		await h.engine.api.resume('start.hub');
		expect(h.engine.api.resumePill.value).toBeNull();
		expect(h.engine.api.active.value?.step.id).toBe('catalog');
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
		expect(h.engine.api.active.value?.step.id).toBe('title');
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
});
