import { createToursStore, normalizeOnboarding } from '../store';
import type { OnboardingApi } from '../api';

const fakeApi = (state: unknown = {}): jest.Mocked<OnboardingApi> => ({
	fetchState: jest.fn().mockResolvedValue(state),
	putTour: jest.fn().mockResolvedValue({}),
	putPreferences: jest.fn().mockResolvedValue({}),
	resetTours: jest.fn().mockResolvedValue({ tours_generation: 3 }),
	fetchFacts: jest.fn().mockResolvedValue({}),
});

const make = (api = fakeApi()) =>
	createToursStore({ api, platform: () => 'web' });

describe('normalizeOnboarding', () => {
	it.each([[[]], [null], [undefined], ['x'], [42]])('malformed %p gives defaults', (raw) => {
		expect(normalizeOnboarding(raw)).toEqual({
			intent: null,
			tips_mode: 'auto',
			hub_hidden: false,
			checklist_hidden: false,
			tours_generation: 0,
			tours: {},
			facts: {},
		});
	});

	it('keeps valid fields and drops invalid ones', () => {
		const state = normalizeOnboarding({
			intent: 'human',
			tips_mode: 'bogus',
			hub_hidden: true,
			tours_generation: 2,
			tours: {
				'a.b': { status: 'paused', step_id: 'x', version: 1, generation: 2 },
				'old.one': { status: 'completed', step_id: null, version: 1, generation: 1 },
				bad: { status: 'nope' },
				worse: 'x',
				legacy: { status: 'paused', step_index: 2, version: 1 },
			},
			facts: { has_task: true },
		});
		expect(state.tips_mode).toBe('auto');
		expect(state.hub_hidden).toBe(true);
		expect(Object.keys(state.tours).sort()).toEqual(['a.b', 'legacy']);
		expect(state.tours.legacy.step_id).toBeNull();
		expect(state.tours.legacy.generation).toBe(2);
		expect(state.facts).toEqual({ has_task: true });
	});
});

describe('store', () => {
	it.each([
		['web', 'web'],
		['mobile_web', 'web'],
		['desktop', 'desktop'],
		['desktop_local', 'desktop'],
		['ios', 'ios'],
	])('sends backend platform for %s as %s', async (client, sent) => {
		const api = fakeApi();
		const store = createToursStore({ api, platform: () => client });
		store.saveTour('a.b', { status: 'paused', step_id: 'x', version: 1 });
		await store.flush();
		expect(api.putTour).toHaveBeenCalledWith('a.b', expect.objectContaining({ platform: sent }));
	});

	it('loads and exposes state; load failure leaves defaults flagged', async () => {
		const store = make(fakeApi({ tips_mode: 'off' }));
		await store.load();
		expect(store.state.value?.tips_mode).toBe('off');
		const failing = fakeApi();
		jest.spyOn(console, 'error').mockImplementation(() => {});
		failing.fetchState.mockRejectedValue(new Error('net'));
		const s2 = make(failing);
		expect(await s2.load()).toBeNull();
		expect(s2.loaded.value).toBe(false);
	});

	it('saves optimistically and PUTs step_id/version/platform/generation', async () => {
		const api = fakeApi({ tours_generation: 2 });
		const store = make(api);
		await store.load();
		const saved = store.saveTour('time.timer', { status: 'in_progress', step_id: 'start', version: 1 });
		expect(store.tourState('time.timer')).toEqual({ status: 'in_progress', step_id: 'start', version: 1, generation: 2 });
		expect(saved?.generation).toBe(2);
		await store.flush();
		expect(api.putTour).toHaveBeenCalledWith('time.timer', {
			status: 'in_progress',
			step_id: 'start',
			version: 1,
			platform: 'web',
			generation: 2,
		});
	});

	it('serialises writes per tour in order', async () => {
		const api = fakeApi();
		const order: string[] = [];
		api.putTour.mockImplementation(async (_id, body) => {
			await new Promise((r) => setTimeout(r, body.status === 'in_progress' ? 20 : 0));
			order.push(body.status);
		});
		const store = make(api);
		store.saveTour('a', { status: 'in_progress', step_id: 's', version: 1 });
		store.saveTour('a', { status: 'paused', step_id: 's', version: 1 });
		await store.flush();
		expect(order).toEqual(['in_progress', 'paused']);
	});

	it('never downgrades a completed tour of the same version', async () => {
		const api = fakeApi({ tours: { a: { status: 'completed', step_id: null, version: 1, generation: 0 } } });
		const store = make(api);
		await store.load();
		expect(store.saveTour('a', { status: 'in_progress', step_id: 's', version: 1 })).toBeNull();
		expect(store.saveTour('a', { status: 'paused', step_id: 's', version: 1 })).toBeNull();
		expect(store.tourState('a')?.status).toBe('completed');
		expect(api.putTour).not.toHaveBeenCalled();
		expect(store.saveTour('a', { status: 'in_progress', step_id: 's', version: 2 })?.status).toBe('in_progress');
		expect(store.saveTour('a', { status: 'completed', step_id: null, version: 2 })?.status).toBe('completed');
	});

	it('drops remote states of an older generation and applies newer ones', async () => {
		const store = make(fakeApi({ tours_generation: 2 }));
		await store.load();
		expect(store.applyRemote('a', { status: 'paused', step_id: 'x', version: 1, generation: 1 })).toBe(false);
		expect(store.tourState('a')).toBeNull();
		expect(store.applyRemote('a', { status: 'paused', step_id: 'x', version: 1, generation: 2 })).toBe(true);
		expect(store.applyRemote('a', { status: 'completed', step_id: null, version: 1, generation: 2 })).toBe(true);
		expect(store.applyRemote('a', { status: 'paused', step_id: 'x', version: 1, generation: 2 })).toBe(false);
		expect(store.applyRemote('a', { status: 'paused', step_id: 'x', version: 1, generation: 5 })).toBe(true);
		expect(store.state.value?.tours_generation).toBe(5);
	});

	it('reload does not move generation backwards', async () => {
		const api = fakeApi({ tours_generation: 4 });
		const store = make(api);
		await store.load();
		api.fetchState.mockResolvedValue({ tours_generation: 1, tours: { z: { status: 'paused', step_id: 'a', version: 1, generation: 1 } } });
		await store.load();
		expect(store.state.value?.tours_generation).toBe(4);
		expect(store.tourState('z')).toBeNull();
	});

	it('resetAll clears tours and takes the new generation', async () => {
		const api = fakeApi({ tours: { a: { status: 'completed', step_id: null, version: 1, generation: 0 } } });
		const store = make(api);
		await store.load();
		await store.resetAll();
		expect(store.state.value?.tours).toEqual({});
		expect(store.state.value?.tours_generation).toBe(3);
	});

	it('setPreferences is optimistic and reverts on failure', async () => {
		const api = fakeApi();
		const store = make(api);
		await store.load();
		await store.setPreferences({ tips_mode: 'off' });
		expect(store.state.value?.tips_mode).toBe('off');
		api.putPreferences.mockRejectedValue(new Error('x'));
		await expect(store.setPreferences({ tips_mode: 'hub_only' })).rejects.toThrow();
		expect(store.state.value?.tips_mode).toBe('off');
	});

	it('works without a successful load', () => {
		const store = make();
		store.saveTour('a', { status: 'paused', step_id: 's', version: 1 });
		expect(store.tourState('a')?.status).toBe('paused');
	});

	it('clear drops state and ignores a load that was in flight', async () => {
		let resolve!: (v: unknown) => void;
		const api = fakeApi();
		api.fetchState.mockReturnValue(new Promise((r) => (resolve = r)));
		const store = make(api);
		const pending = store.load();
		store.clear();
		resolve({ tours_generation: 5 });
		expect(await pending).toBeNull();
		expect(store.state.value).toBeNull();
		expect(store.loaded.value).toBe(false);
	});

	it('does not carry the generation of a previous account after clear', async () => {
		const api = fakeApi({ tours_generation: 4 });
		const store = make(api);
		await store.load();
		store.clear();
		api.fetchState.mockResolvedValue({ tours_generation: 1, tours: { a: { status: 'paused', version: 1, generation: 1 } } });
		await store.load();
		expect(store.state.value?.tours_generation).toBe(1);
		expect(Object.keys(store.state.value?.tours ?? {})).toEqual(['a']);
	});
});
