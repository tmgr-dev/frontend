jest.mock('@/actions/tmgr/daily-tasks', () => ({}));
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

test('patchTaskEntries updates every occurrence of one routine and no other', () => {
	const state = routines.state();
	state.entries = [
		{ task_id: 5, date: '2026-11-09', comments_count: 1 },
		{ task_id: 5, date: '2026-11-10', comments_count: 1 },
		{ task_id: 6, date: '2026-11-10', comments_count: 4 },
	];

	routines.mutations.patchTaskEntries(state, {
		taskId: 5,
		patch: { comments_count: 2 },
	});

	expect(state.entries.map((e: any) => e.comments_count)).toEqual([2, 2, 4]);
});
