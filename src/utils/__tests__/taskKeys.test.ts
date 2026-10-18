import {
	keyPrefixFromPattern,
	onlyCarrier,
	pickTaskByKey,
	taskCarriesKey,
	taskKeyPrefixes,
} from '../taskKeys';

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
		settings: value ? [{ key: 'task_name_pattern_date&time', value }] : [],
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

	it('picks a title stamped without a colon', () => {
		expect(pickTaskByKey([{ id: 4, title: 'TM-241 foo' }], 'TM-241')?.id).toBe(
			4,
		);
	});

	it('falls back to the ticket number of an unstamped task', () => {
		const hits = [
			{ id: 5, title: 'Fix login', category: { code: 'TM' } },
			{
				id: 6,
				title: 'Fix login',
				category: { code: 'TM' },
				category_tasks_sequence_id: 5,
			},
			{ id: 7, title: 'Fix login', category: 3, category_tasks_sequence_id: 5 },
		];
		expect(pickTaskByKey(hits, 'TM-5')?.id).toBe(6);
	});

	it('never takes the ticket number of a task stamped with another key', () => {
		const hits = [
			{ id: 9204, title: 'TM-242 сделать аватарки юзеров кликабельными' },
			{
				id: 9605,
				title: 'TM-406 В Спендли',
				category: { code: 'TM' },
				category_tasks_sequence_id: 242,
			},
		];
		expect(pickTaskByKey(hits, 'TM-242')?.id).toBe(9204);
		expect(pickTaskByKey([hits[1]], 'TM-242')).toBeNull();
	});

	it('picks a title with a space before the colon, not a longer number', () => {
		const hits = [
			{ id: 1, title: 'TM-10: x' },
			{ id: 2, title: 'TM-129 : y' },
		];
		expect(pickTaskByKey(hits, 'TM-129')?.id).toBe(2);
		expect(pickTaskByKey(hits, 'TM-1')).toBeNull();
	});

	it('returns null when two tasks carry the key', () => {
		expect(
			pickTaskByKey(
				[
					{ id: 1, title: 'TM-7 one' },
					{ id: 2, title: 'TM-7: two' },
				],
				'TM-7',
			),
		).toBeNull();
	});
});

describe('taskCarriesKey', () => {
	it('reads the key from the title prefix', () => {
		expect(taskCarriesKey('TM-242: x', null, null, 'TM-242')).toBe(true);
		expect(taskCarriesKey('TM-242 x', null, null, 'TM-242')).toBe(true);
		expect(taskCarriesKey('TM-242\tx', null, null, 'TM-242')).toBe(true);
		expect(taskCarriesKey('TM-242', null, null, 'TM-242')).toBe(true);
		expect(taskCarriesKey('  TM-242: x', null, null, 'TM-242')).toBe(true);
		expect(taskCarriesKey('tm-242 x', null, null, 'TM-242')).toBe(true);
		expect(taskCarriesKey('TM-242 x', null, null, ' tm-242 ')).toBe(true);
	});

	it('does not read a longer number or a mention as the key', () => {
		expect(taskCarriesKey('TM-2420: x', null, null, 'TM-242')).toBe(false);
		expect(taskCarriesKey('TM-242x foo', null, null, 'TM-242')).toBe(false);
		expect(taskCarriesKey('see TM-242', null, null, 'TM-242')).toBe(false);
		expect(taskCarriesKey('TM-242 x', null, null, '')).toBe(false);
	});

	it('uses the ticket number only when the title starts with no key', () => {
		expect(taskCarriesKey('Fix login', 'TM', 5, 'TM-5')).toBe(true);
		expect(taskCarriesKey('Fix login', 'tm', 5, 'TM-5')).toBe(true);
		expect(taskCarriesKey('TM-242x foo', 'TM', 5, 'TM-5')).toBe(true);
		expect(taskCarriesKey('TM-406 В Спендли', 'TM', 242, 'TM-242')).toBe(false);
		expect(taskCarriesKey('TM-406: x', 'TM', 242, 'TM-242')).toBe(false);
		expect(taskCarriesKey('tm-406 x', 'TM', 242, 'TM-242')).toBe(false);
		expect(taskCarriesKey('  TM-406', 'TM', 242, 'TM-242')).toBe(false);
		expect(taskCarriesKey('TM-2420: x', 'TM', 242, 'TM-242')).toBe(false);
		expect(taskCarriesKey('Fix login', 'TM', null, 'TM-5')).toBe(false);
		expect(taskCarriesKey('Fix login', null, 5, 'TM-5')).toBe(false);
		expect(taskCarriesKey('Fix login', 'TM', 6, 'TM-5')).toBe(false);
	});
});

describe('onlyCarrier', () => {
	it('answers only when exactly one item carries the key', () => {
		const even = (n: number) => n % 2 === 0;
		expect(onlyCarrier([1, 2, 3], even)).toBe(2);
		expect(onlyCarrier([1, 3], even)).toBeNull();
		expect(onlyCarrier([2, 4], even)).toBeNull();
	});
});
