import {
	appendText,
	canSubmitDraft,
	hasDraft,
	isScreenshotUnsent,
	isTaskType,
	pickRememberedCategory,
	planOpen,
	readLastCategory,
	submitKind,
	typeAfterScreenshot,
	writeLastCategory,
} from '../quickAddDraft';

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

describe('type rules', () => {
	it('a screenshot moves routine and task drafts to task + screenshot, pages stay', () => {
		expect(typeAfterScreenshot('routine')).toBe('screenshot');
		expect(typeAfterScreenshot('task')).toBe('screenshot');
		expect(typeAfterScreenshot('screenshot')).toBe('screenshot');
		expect(typeAfterScreenshot('page')).toBe('page');
	});

	it('the chosen type alone decides what is created', () => {
		expect(submitKind('routine')).toBe('routine');
		expect(submitKind('task')).toBe('task');
		expect(submitKind('screenshot')).toBe('task-screenshot');
		expect(submitKind('page')).toBe('page');
	});

	it('only task types show the category picker', () => {
		expect(isTaskType('task')).toBe(true);
		expect(isTaskType('screenshot')).toBe(true);
		expect(isTaskType('routine')).toBe(false);
		expect(isTaskType('page')).toBe(false);
	});

	it('task + screenshot needs a screenshot to submit', () => {
		const fields = { title: 'Fix', pageText: '', pageId: null };
		expect(canSubmitDraft('screenshot', fields, false)).toBe(false);
		expect(canSubmitDraft('screenshot', fields, true)).toBe(true);
		expect(canSubmitDraft('task', fields, false)).toBe(true);
		expect(canSubmitDraft('routine', fields, true)).toBe(true);
		expect(canSubmitDraft('routine', { ...fields, title: ' ' }, true)).toBe(
			false,
		);
	});

	it('a page needs a target and text or a screenshot', () => {
		const fields = { title: '', pageText: 'line', pageId: 4 };
		expect(canSubmitDraft('page', fields, false)).toBe(true);
		expect(canSubmitDraft('page', { ...fields, pageText: ' ' }, false)).toBe(
			false,
		);
		expect(canSubmitDraft('page', { ...fields, pageText: ' ' }, true)).toBe(
			true,
		);
		expect(canSubmitDraft('page', { ...fields, pageId: null }, false)).toBe(
			false,
		);
	});

	it('flags a screenshot that the current type would not send', () => {
		expect(isScreenshotUnsent('routine', true)).toBe(true);
		expect(isScreenshotUnsent('task', true)).toBe(true);
		expect(isScreenshotUnsent('screenshot', true)).toBe(false);
		expect(isScreenshotUnsent('page', true)).toBe(false);
		expect(isScreenshotUnsent('task', false)).toBe(false);
	});

	it('a kept screenshot still counts as a draft in any type', () => {
		expect(hasDraft({ ...empty, mode: 'routine' }, true)).toBe(true);
		expect(hasDraft({ ...empty, mode: 'screenshot' }, true)).toBe(true);
		expect(hasDraft({ ...empty, mode: 'screenshot', title: 'x' }, false)).toBe(
			true,
		);
	});
});

describe('remembered category', () => {
	const store = new Map<string, string>();

	beforeEach(() => {
		store.clear();
		(globalThis as any).localStorage = {
			getItem: (k: string) => store.get(k) ?? null,
			setItem: (k: string, v: string) => void store.set(k, v),
		};
	});

	afterEach(() => {
		delete (globalThis as any).localStorage;
	});

	it('is undefined until something is written', () => {
		expect(readLastCategory(1)).toBeUndefined();
	});

	it('remembers an id and No category per workspace', () => {
		writeLastCategory(1, 7);
		writeLastCategory(2, null);
		expect(store.get('desktop.quickAdd.category.1')).toBe('7');
		expect(store.get('desktop.quickAdd.category.2')).toBe('none');
		expect(readLastCategory(1)).toBe(7);
		expect(readLastCategory(2)).toBeNull();
		expect(readLastCategory(3)).toBeUndefined();
	});

	it('falls back to No category for a stale or missing id', () => {
		const categories = [{ id: 7 }, { id: 8 }];
		expect(pickRememberedCategory(categories, 8)).toBe(8);
		expect(pickRememberedCategory(categories, 99)).toBeNull();
		expect(pickRememberedCategory(categories, null)).toBeNull();
		expect(pickRememberedCategory(categories, undefined)).toBeNull();
		expect(pickRememberedCategory([], 7)).toBeNull();
	});

	it('tolerates unavailable storage', () => {
		(globalThis as any).localStorage = {
			getItem: () => {
				throw new Error('blocked');
			},
			setItem: () => {
				throw new Error('blocked');
			},
		};
		expect(readLastCategory(1)).toBeUndefined();
		expect(() => writeLastCategory(1, 2)).not.toThrow();
	});
});
