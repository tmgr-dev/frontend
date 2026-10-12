import { memoryDb, nodeSqliteAvailable } from '../../__tests__/nodeDb';
import { migrate } from '../../schema';
import { readInventory } from '../inventory';

const describeDb = nodeSqliteAvailable ? describe : describe.skip;

describeDb('readInventory', () => {
	it('counts what moves and lists files and personas', async () => {
		const db = memoryDb();
		await migrate(db, '2026-10-12T00:00:00Z');
		const now = '2026-10-12T00:00:00Z';
		await db.execute(
			`INSERT INTO statuses (name, type, sort_order, created_at, updated_at) VALUES ('Todo', 'default', 0, ?, ?)`,
			[now, now],
		);
		await db.execute(
			`INSERT INTO tasks (title, start_time, created_at, updated_at) VALUES ('a', 5, ?, ?), ('b', 0, ?, ?)`,
			[now, now, now, now],
		);
		await db.execute(
			`INSERT INTO tasks (title, created_at, updated_at, deleted_at) VALUES ('gone', ?, ?, ?)`,
			[now, now, now],
		);
		await db.execute(
			`INSERT INTO files (task_id, name, file_path, mime_type, size, created_at) VALUES (1, 'a.png', 'u/a.png', NULL, 3, ?)`,
			[now],
		);
		await db.execute(
			`INSERT INTO personas (uuid, owner_user_id, owner_name, name, synced_at) VALUES ('p1', -1, 'Me', 'Helper', ?), ('p2', 4, 'Me', 'Cloud', ?)`,
			[now, now],
		);
		const inventory = await readInventory(db);
		expect(inventory.counts).toMatchObject({
			tasks: 2,
			files: 1,
			statuses: expect.any(Number),
			pages: expect.any(Number),
		});
		expect(inventory.runningTimers).toBe(1);
		expect(inventory.files).toEqual([
			{ id: 1, name: 'a.png', key: 'u/a.png', size: 3, mime: 'image/png' },
		]);
		expect(inventory.personas.map((p) => [p.uuid, p.ownerUserId])).toEqual([
			['p2', 4],
			['p1', -1],
		]);
		expect(inventory.schemaVersion).toBeGreaterThanOrEqual(12);
	});
});
