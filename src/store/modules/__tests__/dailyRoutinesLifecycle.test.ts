jest.mock('@/actions/tmgr/daily-tasks', () => ({
	expandRoutineRange: jest.fn(),
	expandRoutineYearStats: jest.fn(),
	completeRoutineOn: jest.fn(),
}));
import {
	completeRoutineOn,
	expandRoutineRange,
	expandRoutineYearStats,
} from '@/actions/tmgr/daily-tasks';
import { readFileSync } from 'fs';
import { ModuleKind, transpileModule } from 'typescript';
const source = transpileModule(
	readFileSync(require.resolve('../dailyRoutines.js'), 'utf8'),
	{ compilerOptions: { module: ModuleKind.CommonJS } },
).outputText;
const loaded = { exports: {} as any };
new Function('require', 'module', 'exports', source)(
	require,
	loaded,
	loaded.exports,
);
const routines = loaded.exports.default;
const deferred = () => {
	let resolve!: (v: any) => void;
	const promise = new Promise<any>((a) => {
		resolve = a;
	});
	return { promise, resolve };
};
const context = () => {
	const state = routines.state();
	return {
		state,
		rootState: {
			user: { id: 1 },
			token: { token: 'old' },
			sessionGeneration: 1,
		},
		rootGetters: { currentWorkspaceId: 1 },
		commit: (key: string, payload: any) =>
			(routines.mutations as any)[key](state, payload),
	};
};
test('range and year requests have independent loading and latest responses', async () => {
	const ctx = context(),
		old = deferred(),
		fresh = deferred(),
		year = deferred();
	(expandRoutineRange as jest.Mock)
		.mockReturnValueOnce(old.promise)
		.mockReturnValueOnce(fresh.promise);
	(expandRoutineYearStats as jest.Mock).mockReturnValue(year.promise);
	const a = routines.actions.loadRange(ctx, {
			from: '2025-01-01',
			to: '2025-01-31',
		}),
		b = routines.actions.loadRange(ctx, {
			from: '2026-01-01',
			to: '2026-01-31',
		}),
		c = routines.actions.loadYearStats(ctx, 2026);
	fresh.resolve([{ id: 2 }]);
	await b;
	expect(ctx.state.isLoading).toBe(true);
	old.resolve([{ id: 1 }]);
	await a;
	expect(ctx.state.entries).toEqual([{ id: 2 }]);
	year.resolve({ 2026: 2 });
	await c;
	expect(ctx.state.isLoading).toBe(false);
});
test('range response cannot populate a changed user context', async () => {
	const ctx = context(),
		old = deferred();
	(expandRoutineRange as jest.Mock).mockReturnValue(old.promise);
	const pending = routines.actions.loadRange(ctx, {
		from: '2026-01-01',
		to: '2026-01-31',
	});
	ctx.rootState.user.id = 2;
	old.resolve([{ id: 1 }]);
	await pending;
	expect(ctx.state.entries).toEqual([]);
	expect(ctx.state.isLoading).toBe(false);
});

test.each(['range', 'year'])(
	'%s survives access-token rotation within the same session',
	async (operation) => {
		const ctx = context(),
			response = deferred();
		const fetch =
			operation === 'range' ? expandRoutineRange : expandRoutineYearStats;
		(fetch as jest.Mock).mockReturnValueOnce(response.promise);
		const pending =
			operation === 'range'
				? routines.actions.loadRange(ctx, {
						from: '2026-01-01',
						to: '2026-01-31',
				  })
				: routines.actions.loadYearStats(ctx, 2026);
		ctx.rootState.token = { token: 'refreshed' };
		response.resolve(operation === 'range' ? [{ id: 9 }] : { 2026: 9 });
		await pending;
		expect(
			operation === 'range' ? ctx.state.entries : ctx.state.yearStats,
		).toEqual(operation === 'range' ? [{ id: 9 }] : { 2026: 9 });
		expect(ctx.state.isLoading).toBe(false);
	},
);
test('same-account replacement session rejects an old response', async () => {
	const ctx = context(),
		response = deferred();
	(expandRoutineRange as jest.Mock).mockReturnValueOnce(response.promise);
	const pending = routines.actions.loadRange(ctx, {
		from: '2026-01-01',
		to: '2026-01-31',
	});
	ctx.rootState.sessionGeneration++;
	response.resolve([{ id: 9 }]);
	await pending;
	expect(ctx.state.entries).toEqual([]);
	expect(ctx.state.isLoading).toBe(false);
});

test.each(['refresh', 'session'])(
	'routine mutation ownership handles %s',
	async (change) => {
		const ctx = context(),
			response = deferred();
		ctx.state.entries = [{ task_id: 7, date: '2026-01-01', completed: false }];
		(completeRoutineOn as jest.Mock).mockReturnValueOnce(response.promise);
		const pending = routines.actions.toggleComplete(ctx, ctx.state.entries[0]);
		ctx.rootState.token = { token: 'rotated' };
		if (change === 'session') ctx.rootState.sessionGeneration++;
		response.resolve({ completed: true, instance_id: 2, status: 'done' });
		await pending;
		expect(ctx.state.entries[0].completed).toBe(change === 'refresh');
	},
);
