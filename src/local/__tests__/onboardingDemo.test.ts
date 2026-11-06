import { seedDemo } from '../demoSeed';
import { createOnboardingHandler, type OnboardingStorage } from '../onboarding';
import { migrate } from '../schema';
import type { LocalContext, LocalWorkspace } from '../types';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

const describeDb = nodeSqliteAvailable ? describe : describe.skip;
const NOW = '2026-11-06T10:00:00Z';

const memory = (): OnboardingStorage => {
	const map = new Map<string, string>();
	return {
		getItem: (key) => map.get(key) ?? null,
		setItem: (key, value) => void map.set(key, value),
	};
};

const ctxFor = async (code: string, seeded: boolean): Promise<LocalContext> => {
	const db = memoryDb();
	await migrate(db, NOW);
	const workspace: LocalWorkspace = {
		id: -3,
		name: code,
		code,
		schema_version: 12,
		created_at: '',
		path: `/tmp/${code}`,
		database: `/tmp/${code}/workspace.db`,
	};
	const ctx: LocalContext = {
		db,
		workspace,
		user: { id: 7, name: 'Me', email: 'me@example.com' },
		now: () => new Date(NOW),
		files: {
			url: (key: string) => key,
			read: async () => new Blob([]),
			remove: async () => {},
		},
	};
	if (seeded) await seedDemo(ctx);
	return ctx;
};

describeDb('GET onboarding/demo', () => {
	it('reports the demo workspace and its code', async () => {
		const ctx = await ctxFor('demo', true);
		const handle = createOnboardingHandler(memory(), {
			ctxFor: async () => ctx,
			tokens: async () => [],
		});
		expect(await handle('GET', 'onboarding/demo')).toEqual({
			status: 200,
			data: { data: { is_demo: true, code: 'demo' } },
		});
	});

	it('reports a plain workspace as not a demo', async () => {
		const ctx = await ctxFor('personal', false);
		const handle = createOnboardingHandler(memory(), {
			ctxFor: async () => ctx,
			tokens: async () => [],
		});
		expect(await handle('GET', 'onboarding/demo')).toEqual({
			status: 200,
			data: { data: { is_demo: false, code: 'personal' } },
		});
	});
});
