import { createOnboardingApi } from '../api';

const client = () => ({
	get: jest.fn().mockResolvedValue({ data: { data: { tips_mode: 'auto' } } }),
	put: jest.fn().mockResolvedValue({ data: { data: { ok: 1 } } }),
	delete: jest.fn().mockResolvedValue({ data: { data: { tours_generation: 2 } } }),
});

describe('onboarding api', () => {
	it('unwraps {data} and uses the S1 routes', async () => {
		const c = client();
		const api = createOnboardingApi(c);
		expect(await api.fetchState()).toEqual({ tips_mode: 'auto' });
		expect(c.get).toHaveBeenCalledWith('onboarding');
		const body = { status: 'paused' as const, step_id: 'a', version: 1, platform: 'web', generation: 0 };
		expect(await api.putTour('start.nav', body)).toEqual({ ok: 1 });
		expect(c.put).toHaveBeenCalledWith('onboarding/tours/start.nav', body);
		await api.putPreferences({ tips_mode: 'off' });
		expect(c.put).toHaveBeenCalledWith('onboarding/preferences', { tips_mode: 'off' });
		expect(await api.resetTours()).toEqual({ tours_generation: 2 });
		expect(c.delete).toHaveBeenCalledWith('onboarding/tours');
	});

	it('returns a bare body, including the mock []', async () => {
		const c = client();
		c.get.mockResolvedValue({ data: [] });
		expect(await createOnboardingApi(c).fetchState()).toEqual([]);
	});
});
