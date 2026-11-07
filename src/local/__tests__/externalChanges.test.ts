import type { EventHandlers } from '@/types/dashboard';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
	installExternalChanges,
	type ExternalChangeDeps,
} from '../externalChanges';
import { migrate } from '../schema';
import type { LocalDb } from '../types';
import { nodeSqliteAvailable } from './nodeDb';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

const fileDb = (path: string): LocalDb => {
	// eslint-disable-next-line @typescript-eslint/no-var-requires
	const { DatabaseSync } = require('node:sqlite');
	const db = new DatabaseSync(path);
	db.exec('PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000');
	return {
		async select(sql, params = []) {
			return db.prepare(sql).all(...params);
		},
		async execute(sql, params = []) {
			const result = db.prepare(sql).run(...params);
			return {
				rowsAffected: Number(result.changes),
				lastInsertId: Number(result.lastInsertRowid),
			};
		},
		async batch(statements) {
			const results: { rowsAffected: number; lastInsertId?: number }[] = [];
			db.exec('BEGIN IMMEDIATE');
			try {
				for (const statement of statements) {
					const result = db
						.prepare(statement.sql)
						.run(...(statement.params ?? []));
					results.push({
						rowsAffected: Number(result.changes),
						lastInsertId: Number(result.lastInsertRowid),
					});
				}
				db.exec('COMMIT');
				return { results, failedAt: null };
			} catch (error) {
				db.exec('ROLLBACK');
				throw error;
			}
		},
	};
};

const flush = async () => {
	for (let i = 0; i < 20; i++) await new Promise((r) => setImmediate(r));
};

describeSqlite('installExternalChanges', () => {
	let dir: string;
	let app: LocalDb;
	let cli: LocalDb;
	let emit: (code: string) => void;
	let activeCode: string | null;
	let handlers: Record<
		'onTaskUpdated' | 'onReconnect' | 'onAgentWorkChanged' | 'onCommentUpdated',
		jest.Mock
	>;
	let deps: Record<string, any>;
	let stop: () => void;
	const stamp = new Date().toISOString();

	const addTask = (db: LocalDb, title: string) =>
		db.execute(
			`INSERT INTO tasks (title, created_at, updated_at) VALUES (?, ?, ?)`,
			[title, stamp, stamp],
		);
	const touch = (db: LocalDb, id: number) =>
		db.execute(`UPDATE tasks SET updated_at = ? WHERE id = ?`, [
			new Date(Date.now() + 5000).toISOString(),
			id,
		]);

	beforeEach(async () => {
		jest.useFakeTimers({ doNotFake: ['setImmediate', 'nextTick'] });
		dir = mkdtempSync(join(tmpdir(), 'tmgr-external-'));
		const path = join(dir, 'workspace.db');
		app = fileDb(path);
		await migrate(app, stamp);
		await addTask(app, 'seed one');
		await addTask(app, 'seed two');
		cli = fileDb(path);
		activeCode = 'ws';
		handlers = {
			onTaskUpdated: jest.fn(),
			onReconnect: jest.fn(),
			onAgentWorkChanged: jest.fn(),
			onCommentUpdated: jest.fn(),
		};
		deps = {
			active: () => (activeCode ? { id: -1, code: activeCode } : null),
			subscribeActive: jest.fn(() => () => {}),
			listen: jest.fn(async (handler: (code: string) => void) => {
				emit = handler;
				return () => {};
			}),
			getDb: jest.fn(async () => app),
			fetchTask: jest.fn(async (_w: number, id: number) => ({ id })),
			fetchAgentRuns: jest.fn(async (_w: number, taskId: number) =>
				(
					await app.select<any>(
						'SELECT id, task_id FROM agent_work_runs WHERE task_id = ?',
						[taskId],
					)
				).map((r) => ({ id: r.id, task_id: r.task_id })),
			),
			deliver: jest.fn((_w: number, call: (h: EventHandlers) => void) =>
				call(handlers as EventHandlers),
			),
			deliverToUser: jest.fn((call: (h: EventHandlers) => void) =>
				call(handlers as EventHandlers),
			),
			invalidate: jest.fn(),
			reloadActiveTasks: jest.fn(),
		};
		stop = await installExternalChanges(deps as ExternalChangeDeps);
	});

	afterEach(() => {
		stop();
		jest.useRealTimers();
		rmSync(dir, { recursive: true, force: true });
	});

	it('delivers a task created and one updated by another connection', async () => {
		await addTask(cli, 'from cli');
		await touch(cli, 1);
		emit('ws');
		await flush();
		const calls = handlers.onTaskUpdated.mock.calls.map(([t, a]: any[]) => [
			t.id,
			a,
		]);
		expect(calls).toEqual(
			expect.arrayContaining([
				[1, 'updated'],
				[3, 'created'],
			]),
		);
		expect(deps.invalidate).toHaveBeenCalledWith('task-3');
		expect(deps.invalidate).toHaveBeenCalledWith(/^tasks-status-/);
		expect(handlers.onReconnect).toHaveBeenCalledTimes(2);
	});

	it('delivers a soft-deleted task as deleted without refetching it', async () => {
		await cli.execute(
			`UPDATE tasks SET deleted_at = ?, updated_at = ? WHERE id = 2`,
			[stamp, new Date(Date.now() + 5000).toISOString()],
		);
		emit('ws');
		await flush();
		expect(handlers.onTaskUpdated).toHaveBeenCalledWith({ id: 2 }, 'deleted');
		expect(deps.fetchTask).not.toHaveBeenCalled();
	});

	it('invalidates comments of a task and refetches it when a comment is added', async () => {
		await cli.execute(
			`INSERT INTO comments (task_id, message, created_at, updated_at) VALUES (1, 'hi', ?, ?)`,
			[stamp, stamp],
		);
		emit('ws');
		await flush();
		expect(deps.invalidate).toHaveBeenCalledWith('comments-task-1');
		expect(handlers.onCommentUpdated).toHaveBeenCalledWith({ task_id: 1 });
		expect(deps.fetchTask).toHaveBeenCalledWith(-1, 1);
		expect(handlers.onTaskUpdated).toHaveBeenCalledWith({ id: 1 }, 'updated');
	});

	it('delivers a run finished by another connection', async () => {
		await cli.execute(
			`INSERT INTO agent_work_runs (task_id, agent, status, started_at, actor_kind, actor_id, created_at, updated_at)
			 VALUES (1, 'codex', 'running', ?, 'agent', 'x', ?, ?)`,
			[stamp, stamp, stamp],
		);
		emit('ws');
		await flush();
		expect(handlers.onAgentWorkChanged).toHaveBeenCalledTimes(1);
		await cli.execute(
			`UPDATE agent_work_runs SET status = 'succeeded', version = version + 1, updated_at = ? WHERE id = 1`,
			[new Date(Date.now() + 5000).toISOString()],
		);
		emit('ws');
		await flush();
		expect(handlers.onAgentWorkChanged).toHaveBeenCalledTimes(2);
		expect(handlers.onAgentWorkChanged).toHaveBeenLastCalledWith({
			id: 1,
			task_id: 1,
		});
	});

	it('invalidates statuses and categories caches when they change', async () => {
		await cli.execute(
			`INSERT INTO statuses (name, created_at, updated_at) VALUES ('New', ?, ?)`,
			[stamp, stamp],
		);
		await cli.execute(
			`INSERT INTO categories (title, created_at, updated_at) VALUES ('Cat', ?, ?)`,
			[stamp, stamp],
		);
		emit('ws');
		await flush();
		expect(deps.invalidate).toHaveBeenCalledWith('statuses');
		expect(deps.invalidate).toHaveBeenCalledWith(/^statuses-workspace-/);
		expect(deps.invalidate).toHaveBeenCalledWith('categories');
		expect(deps.invalidate).toHaveBeenCalledWith(/^categories-workspace-/);
	});

	it('uses the bulk path when many tasks changed', async () => {
		for (let i = 0; i < 25; i++) await addTask(cli, `bulk ${i}`);
		emit('ws');
		await flush();
		expect(deps.reloadActiveTasks).toHaveBeenCalledTimes(1);
		expect(deps.invalidate).toHaveBeenCalledWith(/^tasks-status-/);
		expect(deps.fetchTask).not.toHaveBeenCalled();
	});

	it('delivers nothing for an unchanged database', async () => {
		emit('ws');
		emit('ws');
		await flush();
		expect(handlers.onTaskUpdated).not.toHaveBeenCalled();
		expect(handlers.onReconnect).not.toHaveBeenCalled();
		expect(deps.invalidate).not.toHaveBeenCalled();
	});

	it('ignores events of another workspace', async () => {
		await addTask(cli, 'from cli');
		emit('other');
		await flush();
		expect(handlers.onTaskUpdated).not.toHaveBeenCalled();
		expect(deps.getDb).toHaveBeenCalledTimes(1);
	});

	it('throttles onReconnect to once per 10 s with a trailing call', async () => {
		await addTask(cli, 'one');
		emit('ws');
		await flush();
		expect(handlers.onReconnect).toHaveBeenCalledTimes(2);
		await addTask(cli, 'two');
		emit('ws');
		await flush();
		await addTask(cli, 'three');
		emit('ws');
		await flush();
		expect(handlers.onReconnect).toHaveBeenCalledTimes(2);
		jest.advanceTimersByTime(10_000);
		expect(handlers.onReconnect).toHaveBeenCalledTimes(4);
	});

	it('coalesces overlapping events into one run and one rerun', async () => {
		await addTask(cli, 'one');
		let release: () => void = () => {};
		deps.getDb.mockImplementationOnce(
			async () => (await new Promise<void>((r) => (release = r)), app),
		);
		emit('ws');
		emit('ws');
		emit('ws');
		await flush();
		expect(deps.getDb).toHaveBeenCalledTimes(2);
		release();
		await flush();
		expect(deps.getDb).toHaveBeenCalledTimes(3);
		expect(handlers.onTaskUpdated).toHaveBeenCalledTimes(1);
	});

	it('after own edits the first external event refetches them again and still resyncs', async () => {
		await touch(app, 1);
		await addTask(cli, 'from cli');
		emit('ws');
		await flush();
		const ids = handlers.onTaskUpdated.mock.calls
			.map(([t]: any[]) => t.id)
			.sort();
		expect(ids).toEqual([1, 3]);
		handlers.onTaskUpdated.mockClear();
		emit('ws');
		await flush();
		expect(handlers.onTaskUpdated).not.toHaveBeenCalled();
	});

	it('takes a fresh baseline when the active workspace changes', async () => {
		const switched = (deps.subscribeActive as jest.Mock).mock.calls[0][0];
		await addTask(cli, 'before switch');
		switched();
		await flush();
		expect(handlers.onTaskUpdated).not.toHaveBeenCalled();
		await addTask(cli, 'after switch');
		emit('ws');
		await flush();
		expect(handlers.onTaskUpdated).toHaveBeenCalledTimes(1);
	});
});
