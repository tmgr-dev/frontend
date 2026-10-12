import './utcClock';
import fs from 'fs';
import path from 'path';
import { createLocalApi } from '../api';
import { dispatchLocal } from '../dispatch';
import { LATEST_SCHEMA, migrate } from '../schema';
import type { LocalActor, LocalContext, LocalDb } from '../types';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

const PARITY_DIR = path.resolve(__dirname, '../../../crates/tmgr-local/tests/parity');
const SCENARIOS = path.join(PARITY_DIR, 'scenarios');
const GOLDEN = path.join(PARITY_DIR, 'golden');
const UPDATE = process.env.UPDATE_PARITY_GOLDEN === '1';

interface ParityRequest {
	method: string;
	url: string;
	body?: unknown;
	now?: string;
	actor?: LocalActor | null;
}

interface ParityScenario {
	now: string;
	workspace: { id: number; code: string; name: string };
	user: { id: number; name: string; email: string };
	actor: LocalActor | null;
	seed: { sql: string; params?: unknown[] }[];
	requests: ParityRequest[];
}

const sortKeys = (value: unknown): unknown => {
	if (Array.isArray(value)) return value.map(sortKeys);
	if (value && typeof value === 'object') {
		return Object.fromEntries(
			Object.entries(value as Record<string, unknown>)
				.sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
				.map(([k, v]) => [k, sortKeys(v)]),
		);
	}
	return value;
};

const stable = (value: unknown): string => JSON.stringify(sortKeys(JSON.parse(JSON.stringify(value ?? null))), null, '\t') + '\n';

const buildContext = (db: LocalDb, scenario: ParityScenario): LocalContext => ({
	db,
	workspace: {
		...scenario.workspace,
		schema_version: 0,
		created_at: '',
		path: '/tmp/x',
		database: '/tmp/x/workspace.db',
	},
	user: scenario.user,
	now: () => new Date(scenario.now),
	files: {
		url: (key: string) => `tmgrfile://localhost/test/${key}`,
		read: async () => new Blob(['bytes']),
		remove: async () => undefined,
	},
	actor: scenario.actor ?? undefined,
});

const dumpTables = async (db: LocalDb) => {
	const tables = await db.select<{ name: string }>(
		`SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE 'pages_fts%' ORDER BY name`,
	);
	const out: Record<string, unknown[]> = {};
	for (const { name } of tables) out[name] = await db.select(`SELECT * FROM "${name}" ORDER BY rowid`);
	return out;
};

const runScenario = async (scenario: ParityScenario) => {
	const db = memoryDb();
	const ctx = buildContext(db, scenario);
	await migrate(db, scenario.now);
	for (const { sql, params } of scenario.seed) await db.execute(sql, params as any[]);
	const before = await dumpTables(db);
	const api = createLocalApi();
	const responses: unknown[] = [];
	for (const request of scenario.requests) {
		const requestCtx: LocalContext = {
			...ctx,
			now: () => new Date(request.now ?? scenario.now),
			actor: 'actor' in request ? request.actor ?? undefined : ctx.actor,
		};
		const res = await dispatchLocal(api, requestCtx, request.method, request.url, request.body ?? undefined);
		responses.push(res ?? { status: 404, data: null });
	}
	const after = await dumpTables(db);
	const tables = Object.fromEntries(
		Object.entries(after).filter(([name, rows]) => JSON.stringify(rows) !== JSON.stringify(before[name])),
	);
	if (scenario.requests.every((r) => r.method.toUpperCase() === 'GET')) expect(tables).toEqual({});
	return Object.keys(tables).length ? { responses, tables } : { responses };
};

const readJson = (file: string) => JSON.parse(fs.readFileSync(file, 'utf8'));

describeSqlite('tmgr-local parity goldens', () => {
	const names = fs
		.readdirSync(SCENARIOS)
		.filter((f) => f.endsWith('.json'))
		.map((f) => f.slice(0, -5))
		.sort();

	it('has scenarios', () => {
		expect(names.length).toBeGreaterThan(0);
	});

	it('has no golden without a scenario', () => {
		const goldens = fs
			.readdirSync(GOLDEN)
			.filter((f) => f.endsWith('.json') && f !== 'schema.json')
			.map((f) => f.slice(0, -5))
			.sort();
		expect(goldens).toEqual(names);
	});

	it.each(names)('scenario %s matches its golden', async (name) => {
		const actual = JSON.parse(stable(await runScenario(readJson(path.join(SCENARIOS, `${name}.json`)))));
		const file = path.join(GOLDEN, `${name}.json`);
		if (UPDATE) fs.writeFileSync(file, stable(actual));
		expect(actual).toEqual(readJson(file));
	});

	it('schema golden matches sqlite_master, meta and default statuses', async () => {
		const db = memoryDb();
		const now = '2026-09-26T10:00:00Z';
		await migrate(db, now);
		const actual = JSON.parse(
			stable({
				schema_version: LATEST_SCHEMA,
				sqlite_master: await db.select(
					`SELECT type, name, tbl_name, sql FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' AND name NOT IN ('pages_fts_data','pages_fts_idx','pages_fts_docsize','pages_fts_config') ORDER BY rowid`,
				),
				meta: await db.select(`SELECT key, value FROM meta ORDER BY key`),
				statuses: await db.select(`SELECT * FROM statuses ORDER BY id`),
				task_relation_types: await db.select(`SELECT * FROM task_relation_types ORDER BY id`),
			}),
		);
		const file = path.join(GOLDEN, 'schema.json');
		if (UPDATE) fs.writeFileSync(file, stable(actual));
		expect(actual).toEqual(readJson(file));
	});
});
