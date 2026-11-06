import { seedDemo } from '../../demoSeed';
import { memoryDb, nodeSqliteAvailable } from '../../__tests__/nodeDb';
import { migrate } from '../../schema';
import type { LocalContext } from '../../types';
import { createMover, DEMO_MOVE_MESSAGE } from '../flow';
import { readInventory } from '../inventory';
import { harness, INVENTORY } from './helpers';

const describeDb = nodeSqliteAvailable ? describe : describe.skip;
const NOW = '2026-11-06T10:00:00Z';

describeDb('demo workspace and cloud move', () => {
	it('reports the demo flag only for a seeded database', async () => {
		const db = memoryDb();
		await migrate(db, NOW);
		expect((await readInventory(db)).demo).toBe(false);
		const ctx = {
			db,
			workspace: { id: -9, name: 'Demo', code: 'demo' },
			user: { id: 0, name: '', email: '' },
			now: () => new Date(NOW),
		} as unknown as LocalContext;
		await seedDemo(ctx);
		expect((await readInventory(db)).demo).toBe(true);
	});

	it('blocks a dry run of a demo workspace and refuses to start the move', async () => {
		const h = harness({ inventory: { ...INVENTORY, demo: true } });
		const mover = createMover(h.deps);
		const dry = await mover.dryRun('demo', 'Demo: Launch week');
		expect(dry.blocker).toBe(DEMO_MOVE_MESSAGE);
		await expect(
			mover.start('demo', 'Demo: Launch week', dry, {
				cloudName: 'Cloud copy',
				personas: [],
			}),
		).rejects.toThrow(DEMO_MOVE_MESSAGE);
		expect(h.native.moveFreeze).not.toHaveBeenCalled();
		expect(h.api.createJob).not.toHaveBeenCalled();
	});
});
