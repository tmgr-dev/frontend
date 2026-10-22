import {
	alreadyPrefixed,
	composePatternTitle,
	prefixPreview,
	renderPrefix,
} from '../quickAddTitle';

describe('renderPrefix', () => {
	it('fills the category and workspace indexes', () => {
		expect(
			renderPrefix('TM-{index#category}', { workspace: 90, category: 158 }),
		).toBe('TM-158');
		expect(renderPrefix('W{index#workspace}', { workspace: 90 })).toBe('W90');
	});

	it('renders a date mask without indexes', () => {
		jest.useFakeTimers().setSystemTime(new Date(2026, 9, 22, 8, 5));
		try {
			expect(renderPrefix('{dts#yyyy-mm-dd#dte}')).toBe('2026-10-22');
		} finally {
			jest.useRealTimers();
		}
	});
});

describe('prefixPreview', () => {
	it('adds the colon once', () => {
		expect(prefixPreview('TM-158')).toBe('TM-158:');
		expect(prefixPreview('TM-158:')).toBe('TM-158:');
		expect(prefixPreview('')).toBe('');
	});
});

describe('composePatternTitle', () => {
	const pattern = 'TM-{index#category}';

	it('prefixes a plain title like composeTitle', () => {
		expect(composePatternTitle(pattern, 'TM-158', ' Fix login ')).toBe(
			'TM-158: Fix login',
		);
		expect(composePatternTitle('[{index#category}]:', '[3]:', 'Fix')).toBe(
			'[3]: Fix',
		);
	});

	it('prefixes with a date pattern', () => {
		expect(
			composePatternTitle('{dts#yyyy-mm-dd#dte}', '2026-10-22', 'Standup'),
		).toBe('2026-10-22: Standup');
	});

	it('matches a date mask by shape, not as anything', () => {
		const mask = '{dts#dddd, yyyy-mm-dd hh:ii#dte}';
		expect(alreadyPrefixed(mask, 'Monday, 2026-10-22 08:05: Standup')).toBe(
			true,
		);
		expect(alreadyPrefixed(mask, 'Standup')).toBe(false);
		expect(
			alreadyPrefixed('Daily {dts#yyyy-mm-dd#dte}', 'Daily 2026-10-22 plan'),
		).toBe(true);
	});

	it('matches weekday names, not any word', () => {
		expect(composePatternTitle('{dts#dddd#dte}', 'Thursday', 'Fix login')).toBe(
			'Thursday: Fix login',
		);
		expect(composePatternTitle('{dts#ddd#dte}', 'Thu', 'Fix login')).toBe(
			'Thu: Fix login',
		);
		expect(
			composePatternTitle('{dts#dddd#dte}', 'Thursday', 'Friday: Fix'),
		).toBe('Friday: Fix');
	});

	it('never doubles an existing prefix', () => {
		expect(composePatternTitle(pattern, 'TM-158', 'TM-12: Fix login')).toBe(
			'TM-12: Fix login',
		);
		expect(composePatternTitle(pattern, 'TM-158', 'TM-12 Fix login')).toBe(
			'TM-12 Fix login',
		);
		expect(composePatternTitle(pattern, 'TM-158', 'TM-12')).toBe('TM-12');
		expect(
			composePatternTitle(
				'{dts#yyyy-mm-dd#dte}',
				'2026-10-22',
				'2020-01-01 : Old',
			),
		).toBe('2020-01-01 : Old');
	});

	it('does not treat a longer word as a prefix', () => {
		expect(composePatternTitle(pattern, 'TM-158', 'TM-12abc')).toBe(
			'TM-158: TM-12abc',
		);
		expect(composePatternTitle(pattern, 'TM-158', 'Fix TM-12')).toBe(
			'TM-158: Fix TM-12',
		);
	});

	it('leaves the title alone without a pattern', () => {
		expect(composePatternTitle('', '', ' Fix ')).toBe('Fix');
	});

	it('quotes regex characters in literal parts', () => {
		expect(alreadyPrefixed('(v1.{index#category})', '(v1.5) Fix')).toBe(true);
		expect(alreadyPrefixed('(v1.{index#category})', '(v1x5) Fix')).toBe(false);
	});
});
