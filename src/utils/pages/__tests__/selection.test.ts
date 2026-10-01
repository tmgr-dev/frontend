import { findSelectionInSource, meetingActionLines } from '../selection';

describe('findSelectionInSource', () => {
	it('returns an exact single-line slice', () => {
		const body = 'Intro\n\nCall the bank tomorrow\n';
		expect(findSelectionInSource(body, 'Call the bank')).toEqual({
			text: 'Call the bank',
			start: 7,
			end: 20,
		});
	});

	it('maps rendered text across bold markup and includes the wrappers', () => {
		const body = 'Please **send the report** today';
		const match = findSelectionInSource(body, 'send the report');
		expect(match?.text).toBe('**send the report**');
		expect(body.slice(match!.start, match!.end)).toBe(match!.text);
	});

	it('matches text that crosses markup partially', () => {
		const body = 'Please **send** the report today';
		expect(findSelectionInSource(body, 'send the report')?.text).toBe(
			'**send** the report',
		);
	});

	it('expands a link label to the whole link', () => {
		const body = 'See [TM-5](tmgr://task/5) soon';
		expect(findSelectionInSource(body, 'TM-5')?.text).toBe(
			'[TM-5](tmgr://task/5)',
		);
	});

	it('maps a multi-block selection with a list marker', () => {
		const body = '- first item\n- second item\n';
		expect(findSelectionInSource(body, 'first item\nsecond item')?.text).toBe(
			'first item\n- second item',
		);
	});

	it('tolerates whitespace differences', () => {
		const body = 'one  two\nthree';
		expect(findSelectionInSource(body, 'one two three')?.text).toBe(
			'one  two\nthree',
		);
	});

	it('returns null when the text is not in the source', () => {
		expect(findSelectionInSource('abc', 'xyz')).toBeNull();
		expect(findSelectionInSource('abc', '   ')).toBeNull();
	});
});

describe('meetingActionLines', () => {
	const body = [
		'## Agenda',
		'- topic',
		'',
		'## Action items',
		'- Send report',
		'- [ ] Call Ann',
		'1. Book room',
		'- [TM-1](tmgr://task/1)',
		'- ',
		'### Sub',
		'- nested heading item',
		'## Outcomes',
		'- not an action',
	].join('\n');

	it('lists items under the heading through sub-headings, skipping task links', () => {
		expect(meetingActionLines(body).map((line) => line.text)).toEqual([
			'Send report',
			'Call Ann',
			'Book room',
			'nested heading item',
		]);
	});

	it('matches the Russian heading alias', () => {
		expect(
			meetingActionLines('## Действия\n- Send report').map((line) => line.text),
		).toEqual(['Send report']);
	});

	it('reports zero-based line numbers', () => {
		expect(meetingActionLines(body)[0].line).toBe(4);
	});

	it('ignores headings inside code fences and missing headings', () => {
		expect(meetingActionLines('```\n## Action items\n- x\n```')).toEqual([]);
		expect(meetingActionLines('no headings')).toEqual([]);
	});
});
