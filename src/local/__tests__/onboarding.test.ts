import { createOnboardingHandler, ONBOARDING_KEY, type OnboardingStorage } from '../onboarding';

const memory = (): OnboardingStorage & { map: Map<string, string> } => {
	const map = new Map<string, string>();
	return {
		map,
		getItem: (key) => map.get(key) ?? null,
		setItem: (key, value) => void map.set(key, value),
	};
};

const empty = {
	intent: null,
	tips_mode: 'all',
	hub_hidden: false,
	checklist_hidden: false,
	tours_generation: 0,
	tours: {},
	facts: {},
};

describe('local onboarding handler', () => {
	it('answers GET with the default state wrapped in data', () => {
		const handle = createOnboardingHandler(memory());
		expect(handle('GET', 'onboarding')).toEqual({ status: 200, data: { data: empty } });
	});

	it('persists preferences and ignores unknown values', () => {
		const storage = memory();
		const handle = createOnboardingHandler(storage);
		handle('PUT', 'onboarding/preferences', { tips_mode: 'off', hub_hidden: true, bogus: 1 });
		handle('PUT', 'onboarding/preferences', { tips_mode: 'nonsense', checklist_hidden: true });
		const state = createOnboardingHandler(storage)('GET', 'onboarding').data.data;
		expect(state).toMatchObject({ tips_mode: 'off', hub_hidden: true, checklist_hidden: true });
		expect(JSON.parse(storage.map.get(ONBOARDING_KEY)!).tips_mode).toBe('off');
	});

	it('stores tour progress by step id', () => {
		const handle = createOnboardingHandler(memory());
		const res = handle('PUT', 'onboarding/tours/start.first-task', {
			status: 'in_progress',
			step_id: 'create',
			version: 1,
			generation: 0,
			platform: 'web',
		});
		expect(res.status).toBe(200);
		expect(res.data.data.tours['start.first-task']).toEqual({
			status: 'in_progress',
			step_id: 'create',
			version: 1,
			generation: 0,
		});
	});

	it('rejects an invalid tour status', () => {
		const handle = createOnboardingHandler(memory());
		expect(handle('PUT', 'onboarding/tours/a', { status: 'weird', version: 1 }).status).toBe(422);
	});

	it('never downgrades a completed tour with an older generation or version', () => {
		const handle = createOnboardingHandler(memory());
		handle('PUT', 'onboarding/tours/a', { status: 'completed', step_id: null, version: 2, generation: 1 });
		handle('PUT', 'onboarding/tours/a', { status: 'in_progress', step_id: 's', version: 1, generation: 1 });
		const state = handle('GET', 'onboarding').data.data;
		expect(state.tours.a.status).toBe('completed');
	});

	it('drops writes from an older generation than the reset one', () => {
		const handle = createOnboardingHandler(memory());
		handle('PUT', 'onboarding/tours/a', { status: 'completed', step_id: null, version: 1, generation: 0 });
		const reset = handle('DELETE', 'onboarding/tours');
		expect(reset.data.data.tours).toEqual({});
		expect(reset.data.data.tours_generation).toBe(1);
		handle('PUT', 'onboarding/tours/b', { status: 'in_progress', step_id: 'x', version: 1, generation: 0 });
		expect(handle('GET', 'onboarding').data.data.tours).toEqual({});
		handle('PUT', 'onboarding/tours/b', { status: 'in_progress', step_id: 'x', version: 1, generation: 1 });
		expect(handle('GET', 'onboarding').data.data.tours.b.status).toBe('in_progress');
	});

	it('sets the intent and validates it', () => {
		const handle = createOnboardingHandler(memory());
		expect(handle('POST', 'onboarding/intent', { intent: 'agent' }).data.data.intent).toBe('agent');
		expect(handle('POST', 'onboarding/intent', { intent: 'x' }).status).toBe(422);
	});

	it('serves empty facts and a no-op starter task removal', () => {
		const handle = createOnboardingHandler(memory());
		expect(handle('GET', 'onboarding/facts')).toEqual({ status: 200, data: { data: {} } });
		expect(handle('DELETE', 'onboarding/starter-tasks').status).toBe(200);
	});

	it('survives corrupt storage and unknown routes', () => {
		const storage = memory();
		storage.setItem(ONBOARDING_KEY, '{not json');
		const handle = createOnboardingHandler(storage);
		expect(handle('GET', 'onboarding').data.data).toEqual(empty);
		expect(handle('GET', 'onboarding/nope').status).toBe(404);
	});
});
