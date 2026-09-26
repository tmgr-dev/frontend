import {
	buildTrayState,
	formatAway,
	rememberRecent,
	toTrayTask,
} from '../desktopTray';

const api = (id: number, extra: Record<string, unknown> = {}) => ({
	id,
	title: `TM-${id}: task ${id}`,
	common_time: 60,
	start_time: 1_000,
	...extra,
});

it('maps an API task to the tray payload shape', () => {
	expect(toTrayTask(api(7, { workspace_id: 56 }))).toEqual({
		id: 7,
		title: 'TM-7: task 7',
		commonTime: 60,
		startTime: 1_000,
		workspaceId: 56,
	});
	expect(toTrayTask({ id: 8, title: 'x' })).toEqual({
		id: 8,
		title: 'x',
		commonTime: 0,
		startTime: 0,
		workspaceId: null,
	});
});

it('puts running tasks first in the recent list without duplicates', () => {
	const previous = [toTrayTask(api(1)), toTrayTask(api(2))];
	const recent = rememberRecent(previous, [api(2), api(3)]);
	expect(recent.map((t) => t.id)).toEqual([2, 3, 1]);
});

it('keeps the freshest title for a remembered task', () => {
	const previous = [toTrayTask(api(1))];
	const recent = rememberRecent(previous, [api(1, { title: 'TM-1: renamed' })]);
	expect(recent[0].title).toBe('TM-1: renamed');
});

it('caps the recent list', () => {
	const previous = [1, 2, 3, 4, 5].map((id) => toTrayTask(api(id)));
	const recent = rememberRecent(previous, [api(6)], 5);
	expect(recent.map((t) => t.id)).toEqual([6, 1, 2, 3, 4]);
});

it('builds tray state with running timers and remembered tasks', () => {
	const state = buildTrayState([api(2)], [toTrayTask(api(1))]);
	expect(state.running.map((t) => t.id)).toEqual([2]);
	expect(state.recent.map((t) => t.id)).toEqual([1]);
});

it('formats away time for the prompt', () => {
	expect(formatAway(59)).toBe('1 min');
	expect(formatAway(23 * 60 + 10)).toBe('23 min');
	expect(formatAway(2 * 3600 + 5 * 60)).toBe('2 h 5 min');
});

it('keeps a cloud and a local task with the same id apart', () => {
	const recent = rememberRecent(
		[toTrayTask(api(5, { workspace_id: 56 }))],
		[api(5, { workspace_id: -42 })],
	);
	expect(recent.map((t) => [t.id, t.workspaceId])).toEqual([
		[5, -42],
		[5, 56],
	]);
});
