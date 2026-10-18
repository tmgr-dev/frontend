import {
	keyPrefixFromPattern,
	onlyCarrier,
	pickTaskByKey,
	taskCarriesKey,
	taskKeyFrom,
	taskKeyPrefixes,
	titleKey,
	withoutTitleKey,
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

	it('picks a title stamped without a colon in its own category', () => {
		const hit = { id: 4, title: 'TM-241 foo', category: { code: 'TM' } };
		expect(pickTaskByKey([hit], 'TM-241')?.id).toBe(4);
		expect(
			pickTaskByKey([{ id: 4, title: 'TM-241 foo' }], 'TM-241'),
		).toBeNull();
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
			{
				id: 9204,
				title: 'TM-242 сделать аватарки юзеров кликабельными',
				category: { code: 'TM' },
				category_tasks_sequence_id: 78,
			},
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
					{ id: 1, title: 'TM-7 one', category: { code: 'TM' } },
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
		expect(taskCarriesKey('TM-242 x', 'TM', null, 'TM-242')).toBe(true);
		expect(taskCarriesKey('TM-242', 'TM', null, 'TM-242')).toBe(true);
		expect(taskCarriesKey('  TM-242: x', null, null, 'TM-242')).toBe(true);
		expect(taskCarriesKey('tm-242 x', 'tm', null, 'TM-242')).toBe(true);
		expect(taskCarriesKey('TM-242 x', 'TM', null, ' tm-242 ')).toBe(true);
	});

	it('does not read a longer number or a mention as the key', () => {
		expect(taskCarriesKey('TM-2420: x', null, null, 'TM-242')).toBe(false);
		expect(taskCarriesKey('TM-242x foo', 'TM', null, 'TM-242')).toBe(false);
		expect(taskCarriesKey('see TM-242', 'TM', null, 'TM-242')).toBe(false);
		expect(taskCarriesKey('TM-242 x', 'TM', null, '')).toBe(false);
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

	it('takes a key only after ASCII spaces and before a colon, a space or the end', () => {
		expect(taskCarriesKey(' TM-242 x', 'TM', null, 'TM-242')).toBe(true);
		expect(taskCarriesKey('TM-242 x', 'TM', null, 'TM-242')).toBe(false);
		expect(taskCarriesKey('TM-242\tx', 'TM', null, 'TM-242')).toBe(false);
		expect(taskCarriesKey('\tTM-242 x', 'TM', null, 'TM-242')).toBe(false);
	});

	it('treats a key followed by NBSP or tab, or after a tab, as no stamp', () => {
		expect(taskCarriesKey('TM-242 x', 'TM', 5, 'TM-5')).toBe(true);
		expect(taskCarriesKey('TM-242\tx', 'TM', 5, 'TM-5')).toBe(true);
		expect(taskCarriesKey('\tTM-242 x', 'TM', 5, 'TM-5')).toBe(true);
		expect(taskCarriesKey(' TM-242 x', 'TM', 5, 'TM-5')).toBe(false);
	});

	it('takes a spaced key only in its own category, a colon key in any', () => {
		expect(taskCarriesKey('UTF-8 encoding bug', 'TM', 5, 'TM-5')).toBe(true);
		expect(taskCarriesKey('UTF-8 encoding bug', 'TM', 5, 'UTF-8')).toBe(false);
		expect(taskCarriesKey('TMBE-3 x', 'TM', 7, 'TM-7')).toBe(true);
		expect(taskCarriesKey('TMBE-3 x', 'TM', 7, 'TMBE-3')).toBe(false);
		expect(taskCarriesKey('TM-5: x', 'TMBE', 2, 'TM-5')).toBe(true);
		expect(taskCarriesKey('TM-5: x', 'TMBE', 2, 'TMBE-2')).toBe(false);
		expect(taskCarriesKey('TM-9 x', null, null, 'TM-9')).toBe(false);
		expect(taskCarriesKey('TM-9 x', ' ', null, 'TM-9')).toBe(false);
		expect(taskCarriesKey('TM-9: x', null, null, 'TM-9')).toBe(true);
	});
});

describe('titleKey and withoutTitleKey', () => {
	it('read and strip the stamp the matcher reads', () => {
		expect(titleKey('tm-242 example', 'TM')).toBe('TM-242');
		expect(titleKey('  TM-242: example')).toBe('TM-242');
		expect(titleKey('TM-242 : example')).toBe('TM-242');
		expect(titleKey('TM-242', 'tm')).toBe('TM-242');
		expect(titleKey('TM-242 example')).toBeNull();
		expect(titleKey('TM-242 example', 'TMBE')).toBeNull();
		expect(titleKey('TM-242 example', 'TM')).toBeNull();
		expect(titleKey('\tTM-242 example', 'TM')).toBeNull();
		expect(titleKey('TM-242x', 'TM')).toBeNull();
		expect(titleKey(null)).toBeNull();
		expect(withoutTitleKey('TM-242 example', 'TM')).toBe('example');
		expect(withoutTitleKey('TM-242 example')).toBe('TM-242 example');
		expect(withoutTitleKey('UTF-8 encoding bug', 'TM')).toBe(
			'UTF-8 encoding bug',
		);
		expect(withoutTitleKey('TM-242: example')).toBe('example');
		expect(withoutTitleKey(' TM-242 : example')).toBe('example');
		expect(withoutTitleKey('TM-242 example', 'TM')).toBe('TM-242 example');
		expect(withoutTitleKey('Fix login')).toBe('Fix login');
	});
});

describe('taskKeyFrom', () => {
	it('shows the stamped key, else code and ticket number, else nothing', () => {
		expect(taskKeyFrom('TM-242 example', 'TM', 78)).toBe('TM-242');
		expect(taskKeyFrom('TM-242: example', 'TM', 78)).toBe('TM-242');
		expect(taskKeyFrom('Fix login', 'TM', 5)).toBe('TM-5');
		expect(taskKeyFrom('UTF-8 encoding bug', 'TM', 5)).toBe('TM-5');
		expect(taskKeyFrom('TMBE-3 x', 'TM', 7)).toBe('TM-7');
		expect(taskKeyFrom('TM-5: x', 'TMBE', 2)).toBe('TM-5');
		expect(taskKeyFrom('TM-9 x', null, null)).toBeNull();
		expect(taskKeyFrom('TM-9: x', null, null)).toBe('TM-9');
		expect(taskKeyFrom('TM-242 example', 'TM', 78)).toBe('TM-78');
		expect(taskKeyFrom('Fix login', ' ', 5)).toBeNull();
		expect(taskKeyFrom('Fix login', 'TM', null)).toBeNull();
	});

	it('round-trips: every shown key resolves back to its task', () => {
		const tasks: [string, string | null, number | null][] = [
			['TM-242 example', 'TM', 78],
			['TM-242: example', 'TM', 78],
			['  TM-9:', null, null],
			['Fix login', 'TM', 5],
			['UTF-8 encoding bug', 'TM', 5],
			['TMBE-3 x', 'TM', 7],
			['TM-5: x', 'TMBE', 2],
			['TM-1 x', 'TM', 3],
			['\tTM-1 x', 'TM', 4],
		];
		for (const [title, code, seq] of tasks) {
			const key = taskKeyFrom(title, code, seq)!;
			expect(taskCarriesKey(title, code, seq, key)).toBe(true);
		}
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
