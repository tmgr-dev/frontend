import { memoryDb, nodeSqliteAvailable } from '@/local/__tests__/nodeDb';
import { createLocalApi } from '@/local/api';
import {
	DEMO_AGENT,
	finishGhostRun,
	recoverDemo,
	resetDemoCard,
	seedDemo,
} from '@/local/demoSeed';
import { dispatchLocal } from '@/local/dispatch';
import { computeFacts } from '@/local/onboardingFacts';
import { migrate } from '@/local/schema';
import type { LocalContext } from '@/local/types';
import {
	buildScenario,
	createGhostPlayer,
	type GhostClock,
	type GhostEnv,
	type GhostStep,
} from '../ghost';

const fakeClock = () => {
	let now = 0;
	const timers: { at: number; fn: () => void; id: number }[] = [];
	let next = 1;
	const clock: GhostClock = {
		now: () => now,
		setTimeout: (fn, ms) => {
			const id = next++;
			timers.push({ at: now + ms, fn, id });
			return id;
		},
		clearTimeout: (id) => {
			const i = timers.findIndex((t) => t.id === id);
			if (i >= 0) timers.splice(i, 1);
		},
	};
	const advance = async (ms: number) => {
		const target = now + ms;
		for (;;) {
			timers.sort((a, b) => a.at - b.at);
			const due = timers[0];
			if (!due || due.at > target) break;
			timers.shift();
			now = due.at;
			due.fn();
			await Promise.resolve();
			await Promise.resolve();
		}
		now = target;
		await Promise.resolve();
		await Promise.resolve();
	};
	return { clock, advance, pending: () => timers.length };
};

const stepsLog = () => {
	const log: string[] = [];
	const steps: GhostStep[] = [
		{ atMs: 100, run: async () => void log.push('a') },
		{ atMs: 300, run: async () => void log.push('b') },
		{ atMs: 600, run: async () => void log.push('c') },
	];
	return { log, steps };
};

describe('createGhostPlayer', () => {
	it('runs the steps in order at their times', async () => {
		const { clock, advance } = fakeClock();
		const { log, steps } = stepsLog();
		const player = createGhostPlayer({ steps, clock });
		const done = player.start();
		await advance(99);
		expect(log).toEqual([]);
		await advance(1);
		expect(log).toEqual(['a']);
		await advance(200);
		expect(log).toEqual(['a', 'b']);
		await advance(300);
		expect(await done).toBe('completed');
		expect(log).toEqual(['a', 'b', 'c']);
		expect(player.phase()).toBe('finished');
	});

	it('skip runs the remaining steps at once and is idempotent', async () => {
		const { clock, advance } = fakeClock();
		const { log, steps } = stepsLog();
		const player = createGhostPlayer({ steps, clock });
		const done = player.start();
		await advance(150);
		player.skip();
		player.skip();
		expect(await done).toBe('skipped');
		expect(log).toEqual(['a', 'b', 'c']);
		player.skip();
		expect(log).toEqual(['a', 'b', 'c']);
	});

	it('abort stops later steps and leaves no timer behind', async () => {
		const { clock, advance, pending } = fakeClock();
		const { log, steps } = stepsLog();
		const player = createGhostPlayer({ steps, clock });
		const done = player.start();
		await advance(150);
		player.abort();
		expect(await done).toBe('aborted');
		await advance(1000);
		expect(log).toEqual(['a']);
		expect(pending()).toBe(0);
		expect(player.phase()).toBe('aborted');
	});

	it('does not start twice', async () => {
		const { clock, advance } = fakeClock();
		const { log, steps } = stepsLog();
		const player = createGhostPlayer({ steps, clock });
		const first = player.start();
		const second = player.start();
		await advance(1000);
		await Promise.all([first, second]);
		expect(log).toEqual(['a', 'b', 'c']);
	});

	it('reports a failing step and stops', async () => {
		const { clock, advance } = fakeClock();
		const errors: unknown[] = [];
		const ran: string[] = [];
		const player = createGhostPlayer({
			clock,
			onError: (e) => errors.push(e),
			steps: [
				{
					atMs: 10,
					run: async () => {
						throw new Error('boom');
					},
				},
				{ atMs: 20, run: async () => void ran.push('later') },
			],
		});
		const done = player.start();
		await advance(100);
		expect(await done).toBe('failed');
		expect(errors).toHaveLength(1);
		expect(ran).toEqual([]);
	});
});

const describeDb = nodeSqliteAvailable ? describe : describe.skip;
const NOW = '2026-11-06T10:00:00Z';

describeDb('the scenario against a real demo database', () => {
	const setup = async () => {
		const db = memoryDb();
		await migrate(db, NOW);
		const ctx: LocalContext = {
			db,
			workspace: {
				id: -9,
				name: 'Demo',
				code: 'demo',
				schema_version: 0,
				created_at: '',
				path: '/tmp/demo',
				database: '/tmp/demo/workspace.db',
				demo: true,
			},
			user: { id: 0, name: '', email: '' },
			now: () => new Date(NOW),
			accountless: true,
			ghost: true,
			files: {
				url: (key: string) => key,
				read: async () => new Blob([]),
				remove: async () => {},
			},
		};
		await seedDemo(ctx);
		const api = createLocalApi();
		const touched = { tasks: [] as number[], runs: [] as any[], timers: 0 };
		const env: GhostEnv = {
			request: async (method, url, body) => {
				const res = await dispatchLocal(api, ctx, method, url, body);
				if (!res || res.status >= 300) throw new Error(`${method} ${url}`);
				return (res.data as any)?.data;
			},
			taskChanged: (id) => void touched.tasks.push(id),
			runChanged: (run) => void touched.runs.push(run),
			timerChanged: () => void touched.timers++,
		};
		return { ctx, env, touched };
	};

	it('moves the card, plays an agent run and ends in Review with a finished run', async () => {
		const { ctx, env, touched } = await setup();
		const card = (await resetDemoCard(ctx))!;
		const steps = buildScenario(card);
		for (const step of steps) await step.run(env);
		const [row] = await ctx.db.select<any>(
			`SELECT s.name AS status, t.start_time, t.common_time FROM tasks t
			 JOIN statuses s ON s.id = t.status_id WHERE t.id = ?`,
			[card.taskId],
		);
		expect(row).toEqual({ status: 'Review', start_time: 0, common_time: 0 });
		const runs = await ctx.db.select<any>(
			`SELECT agent, status, summary, tests FROM agent_work_runs`,
		);
		expect(runs).toHaveLength(1);
		expect(runs[0].agent).toBe(DEMO_AGENT);
		expect(runs[0].status).toBe('succeeded');
		expect(JSON.parse(runs[0].tests).passed).toBe(14);
		expect(touched.timers).toBe(2);
		expect(touched.runs.length).toBeGreaterThanOrEqual(4);
		expect(new Set(touched.tasks)).toEqual(new Set([card.taskId]));
	});

	it('leaves no fact that counts for real onboarding', async () => {
		const { ctx, env } = await setup();
		const card = (await resetDemoCard(ctx))!;
		for (const step of buildScenario(card)) await step.run(env);
		await finishGhostRun(ctx);
		const facts = await computeFacts(ctx, [
			'has_board_move',
			'has_timer_minute',
			'has_task',
		]);
		expect(facts).toEqual({
			has_board_move: false,
			has_timer_minute: false,
			has_task: true,
		});
		const meta = await ctx.db.select<{ key: string }>(
			`SELECT key FROM meta WHERE key LIKE 'onboarding:%'`,
		);
		expect(meta.map((m) => m.key)).toEqual(['onboarding:demo']);
	});

	it('can be played again after a stop in the middle, then recovered', async () => {
		const { ctx, env } = await setup();
		let card = (await resetDemoCard(ctx))!;
		const steps = buildScenario(card);
		for (const step of steps.slice(0, 4)) await step.run(env);
		expect(await recoverDemo(ctx)).toContain(card.taskId);
		card = (await resetDemoCard(ctx))!;
		for (const step of buildScenario(card)) await step.run(env);
		expect(
			(await ctx.db.select<any>(`SELECT id FROM agent_work_runs`)).length,
		).toBe(1);
	});
});
