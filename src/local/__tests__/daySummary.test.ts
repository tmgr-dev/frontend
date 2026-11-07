import {
	buildDayInput,
	buildMessages,
	dayRange,
	deterministicSummary,
	inRange,
	stripThink,
	type DayTask,
} from '../daySummary';

const task = (over: Partial<DayTask> = {}): DayTask => ({
	workspace: 'Personal',
	key: 'TM-1',
	title: 'Title',
	status: 'In progress',
	createdToday: false,
	touchedToday: false,
	comments: 0,
	commentSnippets: [],
	source: 'local',
	...over,
});

describe('dayRange', () => {
	it('spans the local calendar day of the given instant', () => {
		const range = dayRange(new Date(2026, 10, 7, 15, 30));
		expect(range.from.getTime()).toBe(new Date(2026, 10, 7).getTime());
		expect(range.to.getTime()).toBe(new Date(2026, 10, 8).getTime());
	});

	it('includes the start of the day and excludes the start of the next day', () => {
		const range = dayRange(new Date(2026, 10, 7, 12));
		expect(inRange(new Date(2026, 10, 7, 0, 0).toISOString(), range)).toBe(
			true,
		);
		expect(inRange(new Date(2026, 10, 7, 23, 59).toISOString(), range)).toBe(
			true,
		);
		expect(inRange(new Date(2026, 10, 8, 0, 0).toISOString(), range)).toBe(
			false,
		);
		expect(inRange(new Date(2026, 10, 6, 23, 59).toISOString(), range)).toBe(
			false,
		);
	});

	it('treats an unparsable timestamp as outside the range', () => {
		expect(inRange('not a date', dayRange(new Date(2026, 10, 7)))).toBe(false);
	});
});

describe('buildDayInput', () => {
	it('renders one line with workspace, key, status, flags and minutes', () => {
		const result = buildDayInput([
			task({
				key: 'TM-7',
				title: 'Write docs',
				createdToday: true,
				touchedToday: true,
				comments: 2,
				minutes: 45,
			}),
		]);
		expect(result).toEqual({
			text: '[Personal] TM-7 Write docs | now: In progress | created, commented 2, touched | 45m',
			included: 1,
			omitted: 0,
		});
	});

	it('omits flags and minutes segments when they do not apply', () => {
		const result = buildDayInput([task({ key: 'TM-2', status: 'Todo' })]);
		expect(result.text).toBe('[Personal] TM-2 Title | now: Todo');
	});

	it('includes at most 25 tasks by default and summarises the rest', () => {
		const tasks = Array.from({ length: 40 }, (_, i) =>
			task({ key: `TM-${i}` }),
		);
		const result = buildDayInput(tasks);
		expect(result.included).toBe(25);
		expect(result.omitted).toBe(15);
		const lines = result.text.split('\n');
		expect(lines).toHaveLength(26);
		expect(lines[25]).toBe('and 15 more');
	});

	it('honours an explicit maxTasks', () => {
		const tasks = Array.from({ length: 5 }, (_, i) => task({ key: `TM-${i}` }));
		const result = buildDayInput(tasks, { maxTasks: 2 });
		expect(result.included).toBe(2);
		expect(result.omitted).toBe(3);
		expect(result.text.endsWith('and 3 more')).toBe(true);
	});

	it('ranks by comments, then created, then touched', () => {
		const result = buildDayInput([
			task({ key: 'A', touchedToday: true }),
			task({ key: 'B', comments: 1 }),
			task({ key: 'C', createdToday: true, touchedToday: true }),
			task({ key: 'D', comments: 3 }),
			task({ key: 'E', createdToday: true }),
		]);
		const keys = result.text.split('\n').map((line) => line.split(' ')[1]);
		expect(keys).toEqual(['D', 'B', 'C', 'E', 'A']);
	});

	it('cuts titles longer than 80 characters with an ellipsis', () => {
		const long = buildDayInput([task({ title: 'a'.repeat(100) })]).text;
		expect(long).toContain(`${'a'.repeat(79)}…`);
		expect(long).not.toContain('a'.repeat(80));
		const exact = buildDayInput([task({ title: 'b'.repeat(80) })]).text;
		expect(exact).toContain('b'.repeat(80));
		expect(exact).not.toContain('…');
	});

	it('adds comment snippets cut to 120 characters only for the 10 most commented tasks', () => {
		const tasks = Array.from({ length: 11 }, (_, i) =>
			task({
				key: `T${i}`,
				comments: i + 1,
				commentSnippets: [`snip-${i}-${'x'.repeat(200)}`],
			}),
		);
		const text = buildDayInput(tasks).text;
		expect(text).toContain(`snip-10-${'x'.repeat(120 - 'snip-10-'.length)}`);
		expect(text).not.toContain('x'.repeat(121));
		expect(text).not.toContain('snip-0-');
	});

	it('stops at the token budget and counts dropped tasks as omitted', () => {
		const latin = Array.from({ length: 25 }, (_, i) =>
			task({ key: `L${i}`, title: 'a'.repeat(80) }),
		);
		const cyrillic = Array.from({ length: 25 }, (_, i) =>
			task({ key: `C${i}`, title: 'я'.repeat(80) }),
		);
		const latinResult = buildDayInput(latin, { tokenBudget: 1500 });
		const cyrillicResult = buildDayInput(cyrillic, { tokenBudget: 1500 });
		expect(latinResult.included).toBe(25);
		expect(cyrillicResult.included).toBeLessThan(latinResult.included);
		expect(cyrillicResult.omitted).toBe(25 - cyrillicResult.included);
	});

	it('returns empty text for no tasks', () => {
		expect(buildDayInput([])).toEqual({ text: '', included: 0, omitted: 0 });
	});

	it('never writes "done" or "moved to" for touched tasks', () => {
		const { text } = buildDayInput([
			task({ key: 'TM-9', touchedToday: true, status: 'Review' }),
		]);
		expect(text).not.toMatch(/done/i);
		expect(text).not.toContain('moved to');
		expect(text).toContain('touched');
		expect(text).toContain('now: Review');
	});
});

describe('buildMessages', () => {
	it('asks for a short summary with bullets and open items in the input language', () => {
		const { system } = buildMessages('input');
		expect(system).toContain('short day summary');
		expect(system).toContain('3 to 6 bullet points');
		expect(system).toContain('Open items:');
		expect(system).toContain(
			'reply in the language most of the input is written in',
		);
	});

	it('forbids inventing tasks, times, status transitions or people', () => {
		const { system } = buildMessages('input');
		expect(system).toContain('Do not invent');
		expect(system).toContain('tasks, times, status transitions or people');
		expect(system).not.toContain('moved to');
	});

	it('wraps the input in delimiters inside the prompt', () => {
		const { prompt } = buildMessages('[Personal] TM-1 Title | now: Todo');
		expect(prompt).toContain(
			'<day_input>\n[Personal] TM-1 Title | now: Todo\n</day_input>',
		);
	});
});

describe('stripThink', () => {
	it('removes a closed think block', () => {
		expect(stripThink('<think>plan</think>Summary')).toBe('Summary');
	});

	it('removes multi-line and repeated think blocks and trims the result', () => {
		expect(
			stripThink('  <think>a</think>\n- one\n<think>\nb\nc\n</think>\n- two  '),
		).toBe('- one\n\n- two');
	});

	it('drops an unterminated leading think block entirely', () => {
		expect(stripThink('<think>never closed')).toBe('');
	});

	it('leaves plain text unchanged apart from trimming', () => {
		expect(stripThink('  Plain text \n')).toBe('Plain text');
	});
});

describe('deterministicSummary', () => {
	it('lists counts, total time and task lines', () => {
		const text = deterministicSummary([
			task({
				key: 'TM-1',
				title: 'Title1',
				status: 'Doing',
				createdToday: true,
				comments: 2,
				touchedToday: true,
				minutes: 90,
			}),
			task({
				key: 'TM-2',
				title: 'Title2',
				status: 'Review',
				touchedToday: true,
				minutes: 30,
			}),
		]);
		expect(text).toBe(
			[
				'Created 1',
				'Commented on 1',
				'Touched 2',
				'Time: 2h 0m',
				'TM-1 Title1 (now: Doing)',
				'TM-2 Title2 (now: Review)',
			].join('\n'),
		);
	});

	it('omits zero count lines and the time line when there are no minutes', () => {
		expect(
			deterministicSummary([task({ key: 'TM-3', title: 'X', status: 'Todo' })]),
		).toBe('TM-3 X (now: Todo)');
	});

	it('formats time under an hour as 0h Ym', () => {
		const text = deterministicSummary([
			task({ minutes: 45, touchedToday: true }),
		]);
		expect(text.split('\n')[1]).toBe('Time: 0h 45m');
	});

	it('shows at most 10 task lines', () => {
		const tasks = Array.from({ length: 12 }, (_, i) =>
			task({ key: `TM-${i}`, touchedToday: true }),
		);
		const taskLines = deterministicSummary(tasks)
			.split('\n')
			.filter((line) => line.startsWith('TM-'));
		expect(taskLines).toHaveLength(10);
	});

	it('says nothing was recorded for an empty day', () => {
		expect(deterministicSummary([])).toBe('Nothing recorded today.');
	});

	it('never contains "moved to"', () => {
		const text = deterministicSummary([
			task({ touchedToday: true, status: 'Review' }),
		]);
		expect(text).not.toContain('moved to');
	});
});
