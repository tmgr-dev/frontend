import { titlePatternHandler } from '../titlePatternHandler';

describe('titlePatternHandler date masks', () => {
	const FIXED = new Date(2026, 1, 19, 13, 5, 7); // Thu 2026-02-19 13:05:07
	let nowSpy: jest.SpyInstance;

	beforeEach(() => {
		nowSpy = jest
			.spyOn(global, 'Date')
			.mockImplementation((...args: unknown[]) =>
				args.length
					? new (Date as unknown as new (...a: unknown[]) => Date)(...args)
					: FIXED,
			) as unknown as jest.SpyInstance;
	});

	afterEach(() => {
		nowSpy.mockRestore();
	});

	it('renders ddd as the abbreviated weekday, not a padded day number', () => {
		expect(titlePatternHandler('{dts#dd.mm.yyyy ddd#dte}')).toBe(
			'19.02.2026 Thu',
		);
	});

	it('renders dddd as the full weekday name', () => {
		expect(titlePatternHandler('{dts#dddd#dte}')).toBe('Thursday');
	});

	it('still renders dd as the zero-padded day of month', () => {
		expect(titlePatternHandler('{dts#dd.mm.yyyy#dte}')).toBe('19.02.2026');
	});

	it('renders time tokens correctly alongside date tokens', () => {
		expect(titlePatternHandler('{dts#yyyy-mm-dd hh:ii:ss#dte}')).toBe(
			'2026-02-19 13:05:07',
		);
	});

	it('keeps words before and after a date token', () => {
		expect(titlePatternHandler('Report {dts#dd.mm#dte}')).toBe('Report 19.02');
		expect(titlePatternHandler('Sprint {dts#dd#dte} done')).toBe(
			'Sprint 19 done',
		);
	});

	it('leaves an unfinished or unformattable token as typed', () => {
		expect(titlePatternHandler('Sprint {dts#dd')).toBe('Sprint {dts#dd');
		expect(titlePatternHandler('{dts#note#dte} x')).toBe('{dts#note#dte} x');
	});

	it('substitutes index placeholders', () => {
		expect(
			titlePatternHandler('task {index#category}', new Map([['category', 3]])),
		).toBe('task 3');
	});
});
