import { appendText, hasDraft, planOpen } from '../quickAddDraft';

const empty = { mode: 'task' as const, title: '', note: '', pageText: '' };
const page = { ...empty, mode: 'page' as const };

describe('hasDraft', () => {
	it('is empty for blank fields and no screenshot', () => {
		expect(hasDraft(empty, false)).toBe(false);
		expect(hasDraft({ ...empty, title: '  ', note: '\n' }, false)).toBe(false);
	});

	it('counts any typed field', () => {
		expect(hasDraft({ ...empty, title: 'Fix login' }, false)).toBe(true);
		expect(hasDraft({ ...empty, note: 'details' }, false)).toBe(true);
		expect(hasDraft({ ...page, pageText: 'line' }, false)).toBe(true);
	});

	it('ignores fields hidden in the current mode', () => {
		expect(hasDraft({ ...empty, pageText: 'line' }, false)).toBe(false);
		expect(hasDraft({ ...page, title: 'Fix login', note: 'x' }, false)).toBe(
			false,
		);
		expect(hasDraft(page, true)).toBe(true);
	});

	it('counts a screenshot alone', () => {
		expect(hasDraft(empty, true)).toBe(true);
	});
});

describe('planOpen', () => {
	const none = { hasDraft: false, hasScreenshot: false };
	const typed = { hasDraft: true, hasScreenshot: false };
	const withShot = { hasDraft: true, hasScreenshot: true };

	it('starts fresh when there is no draft', () => {
		expect(planOpen(none, {})).toEqual({
			keep: false,
			text: null,
			screenshot: null,
		});
		expect(planOpen(none, { text: 'hello', screenshot: '/tmp/a.png' })).toEqual(
			{ keep: false, text: 'set', screenshot: 'attach' },
		);
	});

	it('keeps the draft on a plain reopen', () => {
		expect(planOpen(typed, {})).toEqual({
			keep: true,
			text: null,
			screenshot: null,
		});
		expect(planOpen(withShot, {})).toEqual({
			keep: true,
			text: null,
			screenshot: null,
		});
	});

	it('appends selected text to a kept draft instead of replacing it', () => {
		expect(planOpen(typed, { text: 'selected' })).toEqual({
			keep: true,
			text: 'append',
			screenshot: null,
		});
		expect(planOpen(typed, { text: '   ' }).text).toBeNull();
	});

	it('attaches a screenshot to a draft without one', () => {
		expect(planOpen(typed, { screenshot: '/tmp/a.png' })).toEqual({
			keep: true,
			text: null,
			screenshot: 'attach',
		});
	});

	it('only offers a new screenshot when the draft already has one', () => {
		expect(planOpen(withShot, { screenshot: '/tmp/b.png' })).toEqual({
			keep: true,
			text: null,
			screenshot: 'offer',
		});
	});
});

describe('appendText', () => {
	it('separates with a blank line and skips empty parts', () => {
		expect(appendText('first', 'second')).toBe('first\n\nsecond');
		expect(appendText('', ' second ')).toBe('second');
		expect(appendText('first\n', '')).toBe('first');
	});
});
