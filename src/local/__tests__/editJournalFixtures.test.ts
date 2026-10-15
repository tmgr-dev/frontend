import fs from 'fs';
import path from 'path';
import { createLocalApi } from '../api';
import { dispatchLocal } from '../dispatch';
import { confirmTask, provenance, verifyChain } from '../journal';
import { migrate } from '../schema';
import type { LocalActor, LocalContext, LocalDb } from '../types';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

const DIR = path.resolve(
	__dirname,
	'../../../crates/tmgr-local/tests/fixtures/journal',
);
const COLUMNS =
	'seq, at, entity, entity_id, task_id, field, author_kind, source, actor_id, content_sha256, prev_hash, hash';

interface Op {
	writer?: 'rust';
	at: string;
	method?: string;
	url?: string;
	confirm?: number;
	body?: unknown;
	actor?: LocalActor;
	status?: number;
	effects?: string[];
	rows?: Record<string, unknown>[];
}

interface Fixture {
	name: string;
	seed: string[];
	ops: Op[];
	provenance_for: number[];
	expect: {
		journal: Record<string, unknown>[];
		chain: { ok: boolean; broken_at: number | null; rows: number };
		provenance: Record<string, unknown>;
	};
}

const contextFor = (db: LocalDb, op: Op): LocalContext => ({
	db,
	workspace: {
		id: -1,
		name: 'Fixture',
		code: 'fixture',
		schema_version: 13,
		created_at: '',
		path: '/x',
		database: '/x/workspace.db',
	},
	user: { id: 7, name: 'Owner', email: 'owner@example.com' },
	now: () => new Date(op.at),
	files: {
		url: (key) => key,
		read: async () => new Blob(['x']),
		remove: async () => {},
	},
	actor: op.actor,
});

const insertRow = (db: LocalDb, row: Record<string, unknown>) =>
	db.execute(
		`INSERT INTO edit_journal (${COLUMNS}) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		COLUMNS.split(', ').map((column) => row[column]) as any,
	);

describeSqlite('edit journal fixtures shared with the Rust crate', () => {
	const files = fs
		.readdirSync(DIR)
		.filter((file) => file.endsWith('.json'))
		.sort();

	it('has the fixtures', () => {
		expect(files.length).toBeGreaterThanOrEqual(4);
	});

	it.each(files)('%s', async (file) => {
		const fixture: Fixture = JSON.parse(
			fs.readFileSync(path.join(DIR, file), 'utf8'),
		);
		const db = memoryDb();
		await migrate(db, '2026-10-14T00:00:00.000Z');
		for (const statement of fixture.seed) await db.execute(statement);
		const api = createLocalApi();
		for (const op of fixture.ops) {
			if (op.writer === 'rust') {
				for (const effect of op.effects ?? []) await db.execute(effect);
				for (const row of op.rows ?? []) await insertRow(db, row);
				continue;
			}
			if (op.confirm) {
				await confirmTask(contextFor(db, op), op.confirm);
				continue;
			}
			const answer = await dispatchLocal(
				api,
				contextFor(db, op),
				op.method!,
				op.url!,
				op.body,
			);
			if (op.status) expect(answer!.status).toBe(op.status);
			else expect(answer!.status).toBeLessThan(400);
		}
		expect(
			await db.select(`SELECT ${COLUMNS} FROM edit_journal ORDER BY seq`),
		).toEqual(fixture.expect.journal);
		const chain = await verifyChain(db, { full: true });
		expect({
			ok: chain.ok,
			broken_at: chain.brokenAt,
			rows: chain.rows,
		}).toEqual(fixture.expect.chain);
		for (const id of fixture.provenance_for) {
			const got = await provenance(db, id);
			expect(got).toEqual(fixture.expect.provenance[String(id)]);
		}
	});
});
