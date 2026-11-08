import {
	buildCategoryGroups,
	composeTitle,
	fixedSettingsPayload,
	flattenGroups,
	formatDuration,
	insertAt,
	isLiteralPattern,
	literalIssues,
	literalPatternFix,
	parentOptions,
	parseDuration,
	pickCopySource,
	renderTitlePrefix,
	sanitizeCategoryCode,
	TITLE_PATTERN_KEY,
	type CategoryLike,
} from '../categorySettingsPage';

const pattern = (value: string, id = 1) => ({
	id,
	key: TITLE_PATTERN_KEY,
	value,
});
const rate = (value: string) => ({ id: 2, key: 'task_cost_per_hour', value });
const estimate = (value: string) => ({
	id: 3,
	key: 'approximately_time',
	value,
});

const cat = (
	id: number,
	title: string,
	parent: number | null = null,
	settings: CategoryLike['settings'] = [],
	deleted_at: string | null = null,
): CategoryLike => ({
	id,
	title,
	project_category_id: parent,
	settings,
	deleted_at,
});

const PERSONAL = [
	cat(47, 'Common projects', null, [
		pattern('DD.MM.YYYY ddd'),
		estimate('18000'),
	]),
	cat(24, 'Work'),
	cat(27, 'Freelance', 24, [pattern('DD.MM.YYYY')]),
	cat(101, 'Cove', 24, [
		pattern('{dts#DD.MM.YYYY ddd#dte}'),
		rate('700'),
		estimate('28800'),
	]),
	cat(122, 'Morepoke', 27, [pattern('DD.MM.YYYY'), rate('700')]),
	cat(151, 'FCPP', 27, [pattern('{dts#DD.MM.YYYY ddd#dte}'), rate('700')]),
	cat(48, 'Task Manager', 47, [pattern('{dts#DD.MM.YYYY ddd#dte}')]),
	cat(99, 'Gone', 24, [], '2026-10-01T00:00:00Z'),
	cat(200, 'Orphan', 99),
];

describe('category tree', () => {
	it('groups by top-level category, sorts siblings by title and indents by depth', () => {
		const groups = buildCategoryGroups(PERSONAL);
		expect(groups.map((g) => g.root.title)).toEqual([
			'Common projects',
			'Orphan',
			'Work',
		]);
		expect(
			flattenGroups(groups).map(
				(n) => `${'-'.repeat(n.depth)}${n.category.title}`,
			),
		).toEqual([
			'Common projects',
			'-Task Manager',
			'Orphan',
			'Work',
			'-Cove',
			'-Freelance',
			'--FCPP',
			'--Morepoke',
		]);
	});

	it('drops soft-deleted categories and lifts their live children to the top level', () => {
		const titles = flattenGroups(buildCategoryGroups(PERSONAL)).map(
			(n) => n.category.title,
		);
		expect(titles).not.toContain('Gone');
		expect(titles).toContain('Orphan');
	});

	it('survives a parent cycle without looping', () => {
		const groups = buildCategoryGroups([cat(1, 'A', 2), cat(2, 'B', 1)]);
		expect(
			flattenGroups(groups)
				.map((n) => n.category.id)
				.sort(),
		).toEqual([1, 2]);
	});

	it('records ancestors for the crumb', () => {
		const nodes = flattenGroups(buildCategoryGroups(PERSONAL));
		const fcpp = nodes.find((n) => n.category.id === 151)!;
		expect(fcpp.ancestors.map((a) => a.title)).toEqual(['Work', 'Freelance']);
	});
});

describe('parent options', () => {
	it('excludes the category itself and all its descendants', () => {
		const nodes = flattenGroups(buildCategoryGroups(PERSONAL));
		expect(
			parentOptions(nodes, 24)
				.map((n) => n.category.id)
				.sort(),
		).toEqual([200, 47, 48]);
		expect(parentOptions(nodes, null)).toHaveLength(nodes.length);
	});
});

describe('title pattern', () => {
	beforeAll(() => {
		jest.useFakeTimers();
		jest.setSystemTime(new Date(2026, 9, 12, 12));
	});
	afterAll(() => jest.useRealTimers());

	it('renders dates and shows index placeholders as #', () => {
		expect(renderTitlePrefix('{dts#DD.MM.YYYY ddd#dte}')).toBe(
			'12.10.2026 Mon',
		);
		expect(renderTitlePrefix('TM-{index#category}')).toBe('TM-#');
		expect(renderTitlePrefix('TMGR-{index#workspace}:')).toBe('TMGR-#:');
		expect(renderTitlePrefix('DD.MM.YYYY')).toBe('DD.MM.YYYY');
		expect(renderTitlePrefix('')).toBe('');
	});

	it('composes like the backend TaskTitlePattern.compose', () => {
		expect(composeTitle('TM-#', 'Task title')).toBe('TM-#: Task title');
		expect(composeTitle('TC-#:', 'Task title')).toBe('TC-#: Task title');
		expect(composeTitle('', 'Task title')).toBe('Task title');
	});

	it('detects literal patterns and fixes them lower-cased', () => {
		expect(isLiteralPattern('DD.MM.YYYY')).toBe(true);
		expect(isLiteralPattern('{dts#dd.mm.yyyy#dte}')).toBe(false);
		expect(isLiteralPattern('')).toBe(false);
		expect(isLiteralPattern(null)).toBe(false);
		expect(literalPatternFix('DD.MM.YYYY ddd')).toBe(
			'{dts#dd.mm.yyyy ddd#dte}',
		);
		expect(renderTitlePrefix(literalPatternFix('DD.MM.YYYY'))).toBe(
			'12.10.2026',
		);
	});
});

describe('literal issues and fix all', () => {
	it('lists literal patterns in tree order with the ancestor path', () => {
		const issues = literalIssues(flattenGroups(buildCategoryGroups(PERSONAL)));
		expect(issues.map((i) => [i.category.title, i.path, i.pattern])).toEqual([
			['Common projects', '', 'DD.MM.YYYY ddd'],
			['Freelance', 'Work', 'DD.MM.YYYY'],
			['Morepoke', 'Work / Freelance', 'DD.MM.YYYY'],
		]);
	});

	it('keeps every other setting and replaces only the pattern', () => {
		expect(
			fixedSettingsPayload(
				cat(1, 'x', null, [pattern('DD.MM.YYYY', 5), rate('700')]),
			),
		).toEqual([
			{ id: 5, value: '{dts#dd.mm.yyyy#dte}' },
			{ id: 2, value: '700' },
		]);
	});
});

describe('copy settings default', () => {
	const nodes = flattenGroups(buildCategoryGroups(PERSONAL));
	it('picks the first sibling with settings and a non-literal pattern', () => {
		expect(pickCopySource(nodes, 27)?.title).toBe('FCPP');
		expect(pickCopySource(nodes, 24)?.title).toBe('Cove');
	});
	it('returns null when nothing qualifies', () => {
		expect(pickCopySource(nodes, null)).toBeNull();
		expect(pickCopySource(nodes, 101)).toBeNull();
	});
});

describe('helpers', () => {
	it('formats durations', () => {
		expect(formatDuration(28800)).toBe('8h');
		expect(formatDuration('5400')).toBe('1h 30m');
		expect(formatDuration(1200)).toBe('20m');
		expect(formatDuration('')).toBe('');
		expect(formatDuration(0)).toBe('0m');
		expect(formatDuration('0')).toBe('0m');
		expect(formatDuration(null)).toBe('');
	});

	it('sanitizes codes, trimming a trailing hyphen only when final', () => {
		expect(sanitizeCategoryCode('proj x')).toBe('PROJ-X');
		expect(sanitizeCategoryCode('a-')).toBe('A-');
		expect(sanitizeCategoryCode('a-', true)).toBe('A');
		expect(sanitizeCategoryCode('--ab__c')).toBe('AB-C');
	});

	it('inserts a token at the caret', () => {
		expect(insertAt('TM-', '{index#category}', 3)).toEqual({
			value: 'TM-{index#category}',
			caret: 19,
		});
		expect(insertAt('abc', 'X', 1, 2)).toEqual({ value: 'aXc', caret: 2 });
	});
});

describe('parseDuration', () => {
	test('accepts hours, minutes, both, decimals and bare minutes', () => {
		expect(parseDuration('4h')).toBe(14400);
		expect(parseDuration('20m')).toBe(1200);
		expect(parseDuration('1h 30m')).toBe(5400);
		expect(parseDuration('1h30m')).toBe(5400);
		expect(parseDuration('90m')).toBe(5400);
		expect(parseDuration('1.5h')).toBe(5400);
		expect(parseDuration('90')).toBe(5400);
		expect(parseDuration('  1H 30M ')).toBe(5400);
		expect(parseDuration('0')).toBe(0);
	});

	test('returns null for anything else, empty included (the field treats empty as unset)', () => {
		for (const text of [
			'',
			'   ',
			'abc',
			'4x',
			'h',
			'm',
			'-1h',
			'1h 1h',
			'30m 1h',
			'1.5',
			'.5h',
			'1h 30',
		])
			expect(parseDuration(text)).toBeNull();
	});

	test('round-trips formatDuration for whole minutes', () => {
		for (const seconds of [60, 1200, 3600, 5400, 14400, 90060])
			expect(parseDuration(formatDuration(seconds))).toBe(seconds);
	});
});
