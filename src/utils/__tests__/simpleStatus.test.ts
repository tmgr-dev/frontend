import {
	archiveTarget,
	doneTarget,
	isArchived,
	isDone,
	isTaskDone,
	openTarget,
	toggleDoneTarget,
} from '@/utils/simpleStatus';

type S = { id: number; type: string | null; name?: string };

const workspace56: S[] = [
	{ id: 1, type: 'default', name: 'Backlog' },
	{ id: 2, type: 'active', name: 'In progress' },
	{ id: 3, type: 'completed', name: 'Done' },
	{ id: 4, type: 'archived', name: 'Archived' },
];
const legacySeed: S[] = [
	{ id: 10, type: 'default', name: 'Backlog' },
	{ id: 11, type: 'active', name: 'Completed' },
	{ id: 12, type: 'archived', name: 'Archived' },
];
const noDefault: S[] = [
	{ id: 20, type: 'archived' },
	{ id: 21, type: 'hidden' },
	{ id: 22, type: 'completed' },
	{ id: 23, type: 'active' },
];
const onlyClosed: S[] = [
	{ id: 30, type: 'completed' },
	{ id: 31, type: 'archived' },
	{ id: 32, type: 'hidden' },
];
const twoOfEach: S[] = [
	{ id: 40, type: 'active' },
	{ id: 41, type: 'default' },
	{ id: 42, type: 'completed' },
	{ id: 43, type: 'default' },
	{ id: 44, type: 'completed' },
	{ id: 45, type: 'archived' },
	{ id: 46, type: 'archived' },
];

const targetCases: {
	name: string;
	statuses: S[] | null | undefined;
	open: number | null;
	done: number | null;
	archive: number | null;
}[] = [
	{
		name: 'workspace 56 like',
		statuses: workspace56,
		open: 1,
		done: 3,
		archive: 4,
	},
	{
		name: 'legacy seed',
		statuses: legacySeed,
		open: 10,
		done: null,
		archive: 12,
	},
	{
		name: 'no default type',
		statuses: noDefault,
		open: 23,
		done: 22,
		archive: 20,
	},
	{
		name: 'only closed types',
		statuses: onlyClosed,
		open: null,
		done: 30,
		archive: 31,
	},
	{
		name: 'first of each type wins',
		statuses: twoOfEach,
		open: 41,
		done: 42,
		archive: 45,
	},
	{ name: 'empty list', statuses: [], open: null, done: null, archive: null },
	{ name: 'null list', statuses: null, open: null, done: null, archive: null },
	{
		name: 'undefined list',
		statuses: undefined,
		open: null,
		done: null,
		archive: null,
	},
];

describe('simple status targets', () => {
	it.each(targetCases)('$name', ({ statuses, open, done, archive }) => {
		expect(openTarget(statuses)).toBe(open);
		expect(doneTarget(statuses)).toBe(done);
		expect(archiveTarget(statuses)).toBe(archive);
	});
});

describe('status type predicates', () => {
	it('treats only completed as done', () => {
		expect(isDone({ type: 'completed' })).toBe(true);
		expect(isDone({ type: 'active' })).toBe(false);
		expect(isDone({ type: 'archived' })).toBe(false);
		expect(isDone(null)).toBe(false);
		expect(isDone(undefined)).toBe(false);
	});

	it('treats only archived as archived', () => {
		expect(isArchived({ type: 'archived' })).toBe(true);
		expect(isArchived({ type: 'completed' })).toBe(false);
		expect(isArchived(null)).toBe(false);
	});
});

const taskCases: {
	name: string;
	task: { status_id?: number | string | null } | null;
	statuses: S[];
	done: boolean;
	toggle: number | null;
}[] = [
	{
		name: 'open task goes to done',
		task: { status_id: 2 },
		statuses: workspace56,
		done: false,
		toggle: 3,
	},
	{
		name: 'done task goes to open',
		task: { status_id: 3 },
		statuses: workspace56,
		done: true,
		toggle: 1,
	},
	{
		name: 'archived task is not done',
		task: { status_id: 4 },
		statuses: workspace56,
		done: false,
		toggle: 3,
	},
	{
		name: 'task without status is open',
		task: { status_id: null },
		statuses: workspace56,
		done: false,
		toggle: 3,
	},
	{
		name: 'task with unknown status is open',
		task: { status_id: 999 },
		statuses: workspace56,
		done: false,
		toggle: 3,
	},
	{
		name: 'string status id',
		task: { status_id: '3' },
		statuses: workspace56,
		done: true,
		toggle: 1,
	},
	{
		name: 'legacy seed has no done target',
		task: { status_id: 10 },
		statuses: legacySeed,
		done: false,
		toggle: null,
	},
	{
		name: 'legacy seed completed-named task is open',
		task: { status_id: 11 },
		statuses: legacySeed,
		done: false,
		toggle: null,
	},
	{
		name: 'done task without open target',
		task: { status_id: 30 },
		statuses: onlyClosed,
		done: true,
		toggle: null,
	},
	{
		name: 'empty statuses',
		task: { status_id: 1 },
		statuses: [],
		done: false,
		toggle: null,
	},
	{
		name: 'null task',
		task: null,
		statuses: workspace56,
		done: false,
		toggle: 3,
	},
];

describe('task completion', () => {
	it.each(taskCases)('$name', ({ task, statuses, done, toggle }) => {
		expect(isTaskDone(task, statuses)).toBe(done);
		expect(toggleDoneTarget(task, statuses)).toBe(toggle);
	});
});
