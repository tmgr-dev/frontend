import {
	acceptRevision,
	addRevision,
	composeRewriteText,
	createRequestGuard,
	createRewriteSession,
	rewriteErrorMessage,
	selectedText,
	selectRevision,
} from '../aiRewrite';

describe('composeRewriteText', () => {
	it('sends the bare text when there is no instruction', () => {
		expect(composeRewriteText('Fix the login', '')).toBe('Fix the login');
		expect(composeRewriteText('Fix the login', '   ')).toBe('Fix the login');
	});

	it('wraps the base text and the instruction in delimiters', () => {
		const text = composeRewriteText('Fix the login', ' make it shorter ');
		expect(text).toContain('<description>\nFix the login\n</description>');
		expect(text).toContain('<instruction>\nmake it shorter\n</instruction>');
		expect(text.indexOf('<description>')).toBeLessThan(
			text.indexOf('<instruction>'),
		);
	});
});

describe('rewrite session', () => {
	it('keeps the original as revision 0 and selects it', () => {
		const s = createRewriteSession('orig');
		expect(s.revisions).toEqual(['orig']);
		expect(selectedText(s)).toBe('orig');
	});

	it('adds a revision and selects it without touching the original', () => {
		const s1 = addRevision(createRewriteSession('orig'), 'one');
		const s2 = addRevision(s1, 'two');
		expect(s2.revisions).toEqual(['orig', 'one', 'two']);
		expect(selectedText(s2)).toBe('two');
		expect(s1.revisions).toEqual(['orig', 'one']);
	});

	it('selects any revision and ignores out-of-range indexes', () => {
		const s = addRevision(
			addRevision(createRewriteSession('orig'), 'one'),
			'two',
		);
		expect(selectedText(selectRevision(s, 1))).toBe('one');
		expect(selectedText(selectRevision(s, 0))).toBe('orig');
		expect(selectRevision(s, 9)).toBe(s);
		expect(selectRevision(s, -1)).toBe(s);
	});

	it('accepts the selected revision and returns the text to undo to', () => {
		const s = selectRevision(
			addRevision(addRevision(createRewriteSession('orig'), 'one'), 'two'),
			1,
		);
		expect(acceptRevision(s, 'orig')).toEqual({
			next: 'one',
			previous: 'orig',
		});
	});
});

describe('acceptRevision when the description changed meanwhile', () => {
	it('refuses, so newer text is not overwritten', () => {
		const s = addRevision(createRewriteSession('original'), 'one');
		expect(acceptRevision(s, 'edited in the meantime')).toBeNull();
	});
});

describe('createRequestGuard', () => {
	it('treats only the latest request as current', () => {
		const guard = createRequestGuard();
		const first = guard.begin();
		const second = guard.begin();
		expect(guard.isCurrent(first)).toBe(false);
		expect(guard.isCurrent(second)).toBe(true);
	});

	it('drops a pending request after cancel', () => {
		const guard = createRequestGuard();
		const token = guard.begin();
		guard.cancel();
		expect(guard.isCurrent(token)).toBe(false);
	});
});

describe('rewriteErrorMessage', () => {
	const err = (status?: number, data?: unknown) => ({
		response: status ? { status, data } : undefined,
	});
	it.each([
		[422, 'empty'],
		[413, 'too long'],
		[429, 'Too many'],
		[500, 'try again'],
		[undefined, 'connection'],
	])('maps %s', (status, fragment) => {
		expect(rewriteErrorMessage(err(status as number | undefined))).toContain(
			fragment,
		);
	});

	it('reports a disabled module', () => {
		expect(
			rewriteErrorMessage(err(403, { error: 'feature_disabled' })),
		).toContain('turned off');
	});
});
