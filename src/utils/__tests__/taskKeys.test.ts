import { keyPrefixFromPattern, taskKeyPrefixes, pickTaskByKey } from '../taskKeys';

describe('keyPrefixFromPattern', () => {
	it('takes the letters a category puts in front of every task', () => {
		expect(keyPrefixFromPattern('TM-{index#category}:')).toBe('TM');
		expect(keyPrefixFromPattern('TMBE-{index#category}')).toBe('TMBE');
	});

	it('ignores patterns that are only a date', () => {
		expect(keyPrefixFromPattern('YYYY-MM-DD')).toBeNull();
		expect(keyPrefixFromPattern('{dts#YYYY-MM-DD#dte}')).toBeNull();
	});

	it('ignores empty or missing patterns', () => {
		expect(keyPrefixFromPattern('')).toBeNull();
		expect(keyPrefixFromPattern(undefined as unknown as string)).toBeNull();
	});
});

describe('taskKeyPrefixes', () => {
	const category = (value?: string) => ({
		settings: value
			? [{ key: 'task_name_pattern_date&time', value }]
			: [],
	});

	it('collects the prefixes of every category, without repeats', () => {
		expect(
			taskKeyPrefixes([
				category('TM-{index#category}:'),
				category('TMBE-{index#category}:'),
				category('TM-{index#category}'),
				category(),
			]),
		).toEqual(['TM', 'TMBE']);
	});

	it('returns nothing when no category names its tasks', () => {
		expect(taskKeyPrefixes([category(), category('YYYY-MM-DD')])).toEqual([]);
		expect(taskKeyPrefixes(null as never)).toEqual([]);
	});
});

describe('pickTaskByKey', () => {
	const tasks = [
		{ id: 1, title: 'TM-129: the real one' },
		{ id: 2, title: 'TM-1290: a different task' },
		{ id: 3, title: 'mentions TM-129 in passing' },
	];

	it('picks the task whose title starts with the key', () => {
		expect(pickTaskByKey(tasks, 'TM-129')?.id).toBe(1);
	});

	it('matches the key however it was typed', () => {
		expect(pickTaskByKey(tasks, 'tm-129')?.id).toBe(1);
	});

	it('does not settle for a longer number', () => {
		expect(pickTaskByKey([tasks[1]], 'TM-129')).toBeNull();
	});

	it('ignores a task that only mentions the key', () => {
		expect(pickTaskByKey([tasks[2]], 'TM-129')).toBeNull();
	});

	it('returns null when nothing matches', () => {
		expect(pickTaskByKey([], 'TM-129')).toBeNull();
	});
});
