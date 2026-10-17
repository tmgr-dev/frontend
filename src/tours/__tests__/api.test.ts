import { createOnboardingApi } from '../api';

const client = () => ({
	get: jest.fn().mockResolvedValue({ data: { data: { tips_mode: 'auto' } } }),
	post: jest.fn().mockResolvedValue({ data: { data: { ok: 1 } } }),
	put: jest.fn().mockResolvedValue({ data: { data: { ok: 1 } } }),
	delete: jest
		.fn()
		.mockResolvedValue({ data: { data: { tours_generation: 2 } } }),
});

describe('onboarding api', () => {
	it('unwraps {data} and uses the S1 routes', async () => {
		const c = client();
		const api = createOnboardingApi(c);
		expect(await api.fetchState()).toEqual({ tips_mode: 'auto' });
		expect(c.get).toHaveBeenCalledWith('onboarding');
		const body = {
			status: 'paused' as const,
			step_id: 'a',
			version: 1,
			platform: 'web',
			generation: 0,
		};
		expect(await api.putTour('start.nav', body)).toEqual({ ok: 1 });
		expect(c.put).toHaveBeenCalledWith('onboarding/tours/start.nav', body);
		await api.putPreferences({ tips_mode: 'off' });
		expect(c.put).toHaveBeenCalledWith('onboarding/preferences', {
			tips_mode: 'off',
		});
		expect(await api.resetTours()).toEqual({ tours_generation: 2 });
		expect(c.delete).toHaveBeenCalledWith('onboarding/tours');
	});

	it('returns a bare body, including the mock []', async () => {
		const c = client();
		c.get.mockResolvedValue({ data: [] });
		expect(await createOnboardingApi(c).fetchState()).toEqual([]);
	});

	it('posts the intent, adding seed only when given', async () => {
		const c = client();
		c.post.mockResolvedValue({
			data: { data: { intent: 'human', starter_tasks_created: 3 } },
		});
		const api = createOnboardingApi(c);
		expect(await api.setIntent('human')).toEqual({
			intent: 'human',
			starter_tasks_created: 3,
		});
		expect(c.post).toHaveBeenLastCalledWith('onboarding/intent', {
			intent: 'human',
		});
		await api.setIntent('human', { seed: false });
		expect(c.post).toHaveBeenLastCalledWith('onboarding/intent', {
			intent: 'human',
			seed: false,
		});
	});

	it('fetches only the requested facts, comma-joined, with an optional task', async () => {
		const c = client();
		c.get.mockResolvedValue({ data: { data: { has_timer_minute: true } } });
		const api = createOnboardingApi(c);
		expect(await api.fetchFacts(['has_timer_minute', 'has_task'], 9)).toEqual({
			has_timer_minute: true,
		});
		expect(c.get).toHaveBeenLastCalledWith('onboarding/facts', {
			params: { keys: 'has_timer_minute,has_task', task_id: 9 },
		});
		await api.fetchFacts(['has_task']);
		expect(c.get).toHaveBeenLastCalledWith('onboarding/facts', {
			params: { keys: 'has_task' },
		});
	});

	it('returns {} for a non-object facts body', async () => {
		const c = client();
		c.get.mockResolvedValue({ data: [] });
		expect(await createOnboardingApi(c).fetchFacts(['a'])).toEqual({});
	});

	it('removes starter tasks and reports the count', async () => {
		const c = client();
		c.delete.mockResolvedValue({ data: { data: { removed: 3 } } });
		const api = createOnboardingApi(c);
		expect(await api.removeStarterTasks()).toEqual({ removed: 3 });
		expect(c.delete).toHaveBeenCalledWith('onboarding/starter-tasks');
		c.delete.mockResolvedValue({ data: [] });
		expect(await api.removeStarterTasks()).toEqual({ removed: 0 });
	});
});
