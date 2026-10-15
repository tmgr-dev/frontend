import { createLocalApi } from '../api';
import { buildConfirmPreview, confirmShownTask } from '../confirmPreview';
import { dispatchLocal } from '../dispatch';
import { isHumanWritten, provenance } from '../journal';
import { enableLocalPersona } from '../personas';
import { migrate } from '../schema';
import type { LocalActor, LocalContext } from '../types';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

describeSqlite('confirm for agent runs', () => {
	let ctx: LocalContext;
	const api = createLocalApi();
	const persona: LocalActor = { kind: 'persona', id: 'p-1', name: 'Reviewer' };
	const call = (
		method: string,
		url: string,
		body?: unknown,
		actor?: LocalActor,
	) => dispatchLocal(api, { ...ctx, actor }, method, url, body);

	beforeEach(async () => {
		ctx = {
			db: memoryDb(),
			workspace: {
				id: -1,
				name: 'P',
				code: 'local-p',
				schema_version: 0,
				created_at: '',
				path: '/x',
				database: '/x/workspace.db',
			},
			user: { id: 7, name: 'Owner', email: 'me@example.com' },
			now: () => new Date('2026-10-14T10:00:00Z'),
			files: {
				url: (key) => key,
				read: async () => new Blob(['x']),
				remove: async () => {},
			},
		};
		await migrate(ctx.db, '2026-10-14T00:00:00.000Z');
		await ctx.db.execute(
			`INSERT INTO personas (uuid, owner_user_id, owner_name, name, synced_at) VALUES ('p-1', 7, 'Owner', 'Reviewer', 'x')`,
		);
		await enableLocalPersona(ctx, 'p-1', ['tasks:write', 'comments:write']);
	});

	const createTask = async () =>
		(await call('POST', 'tasks', {
			title: 'Do <b>it</b>',
			description: 'Steps\nmore',
			checkpoints: [
				{ description: 'first', checked: true },
				{ description: 'second', checked: false },
			],
		}))!.data.data.id as number;

	it('shows the whole text an agent can read, with author kinds and the short hash', async () => {
		const id = await createTask();
		await call('POST', `tasks/${id}/comments`, { message: 'by me' });
		await call(
			'POST',
			`tasks/${id}/comments`,
			{ message: 'by an agent' },
			persona,
		);
		const preview = (await buildConfirmPreview(ctx.db, id))!;
		expect(preview.title).toBe('Do <b>it</b>');
		expect(preview.description).toBe('Steps\nmore');
		expect(preview.checkpoints).toEqual(['[x] first', '[ ] second']);
		expect(JSON.parse(preview.checkpointsText)).toEqual([
			{ description: 'first', checked: true },
			{ description: 'second', checked: false },
		]);
		expect(preview.checkpointsText).toContain('\n  ');
		expect(preview.comments.map((c) => [c.author_kind, c.message])).toEqual([
			['user', 'by me'],
			['persona', 'by an agent'],
		]);
		expect(preview.shortHash).toBe(preview.snapshotHash.slice(0, 12));
		expect(preview.snapshotHash).toMatch(/^[0-9a-f]{64}$/);
		expect(await buildConfirmPreview(ctx.db, 999)).toBeNull();
	});

	it('confirms the shown text; the task then counts as human even after an agent wrote parts of it', async () => {
		const id = await createTask();
		await call(
			'PATCH',
			`tasks/${id}`,
			{ description: 'agent rewrite' },
			persona,
		);
		expect(isHumanWritten(await provenance(ctx.db, id))).toBe(false);
		const preview = (await buildConfirmPreview(ctx.db, id))!;
		await confirmShownTask(ctx, preview);
		expect(isHumanWritten(await provenance(ctx.db, id))).toBe(true);
	});

	it('refuses when the task changed after it was shown', async () => {
		const id = await createTask();
		const preview = (await buildConfirmPreview(ctx.db, id))!;
		await call(
			'POST',
			`tasks/${id}/comments`,
			{ message: 'sneaked in' },
			persona,
		);
		const before = (await ctx.db.select(`SELECT seq FROM edit_journal`)).length;
		await expect(confirmShownTask(ctx, preview)).rejects.toThrow(/changed/);
		expect((await ctx.db.select(`SELECT seq FROM edit_journal`)).length).toBe(
			before,
		);
		expect(isHumanWritten(await provenance(ctx.db, id))).toBe(false);
	});

	it('is not available to an actor', async () => {
		const id = await createTask();
		const preview = (await buildConfirmPreview(ctx.db, id))!;
		await expect(
			confirmShownTask({ ...ctx, actor: persona }, preview),
		).rejects.toThrow();
	});
});
