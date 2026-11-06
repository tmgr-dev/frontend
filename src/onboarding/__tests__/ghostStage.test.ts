import { ONBOARDING_KEY } from '@/local/onboarding';
import { readGhostStage, writeGhostStage } from '../ghostStage';

const memory = () => {
	const map = new Map<string, string>();
	return {
		map,
		getItem: (key: string) => map.get(key) ?? null,
		setItem: (key: string, value: string) => void map.set(key, value),
	};
};

describe('ghost stage', () => {
	it('is absent for an existing user', () => {
		const storage = memory();
		expect(readGhostStage(storage)).toBeNull();
		storage.map.set(ONBOARDING_KEY, JSON.stringify({ intent: 'human' }));
		expect(readGhostStage(storage)).toBeNull();
	});

	it('is stored inside the onboarding state without touching the rest', () => {
		const storage = memory();
		storage.map.set(
			ONBOARDING_KEY,
			JSON.stringify({ intent: 'human', tours: { a: 1 } }),
		);
		writeGhostStage(storage, 'wheel');
		expect(JSON.parse(storage.map.get(ONBOARDING_KEY)!)).toEqual({
			intent: 'human',
			tours: { a: 1 },
			ghost_stage: 'wheel',
		});
		expect(readGhostStage(storage)).toBe('wheel');
	});

	it('ignores an unknown stage and corrupt state', () => {
		const storage = memory();
		storage.map.set(ONBOARDING_KEY, JSON.stringify({ ghost_stage: 'later' }));
		expect(readGhostStage(storage)).toBeNull();
		storage.map.set(ONBOARDING_KEY, '{nope');
		expect(readGhostStage(storage)).toBeNull();
		writeGhostStage(storage, 'watch');
		expect(readGhostStage(storage)).toBe('watch');
	});

	it('survives a later local onboarding write that loads the whole state', async () => {
		const { createOnboardingHandler } = await import('@/local/onboarding');
		const storage = memory();
		writeGhostStage(storage, 'wheel');
		const handler = createOnboardingHandler(storage);
		await handler('PUT', 'onboarding/preferences', { hub_hidden: true });
		expect(readGhostStage(storage)).toBe('wheel');
	});

	it('keeps a stage written while the intent route is still seeding', async () => {
		const { createOnboardingHandler } = await import('@/local/onboarding');
		const storage = memory();
		writeGhostStage(storage, 'watch');
		const ctxFor = jest.fn(async () => {
			writeGhostStage(storage, 'wheel');
			return null;
		});
		const handler = createOnboardingHandler(storage, {
			ctxFor,
			tokens: async () => [],
		});
		await handler('POST', 'onboarding/intent', { intent: 'human', seed: true });
		expect(ctxFor).toHaveBeenCalled();
		expect(readGhostStage(storage)).toBe('wheel');
		expect(JSON.parse(storage.map.get(ONBOARDING_KEY)!).intent).toBe('human');
	});
});
