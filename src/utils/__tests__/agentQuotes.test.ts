import {
	addQuote,
	draftChip,
	findQuoteSpan,
	MAX_QUOTE_TEXT,
	MAX_QUOTES,
	removeQuote,
	sentChip,
	truncateQuote,
} from '../agentQuotes';

describe('addQuote', () => {
	it('appends whole-message and fragment quotes', () => {
		let q = addQuote([], { message_id: 1, text: null });
		q = addQuote(q, { message_id: 2, text: '  part  ' });
		expect(q).toEqual([
			{ message_id: 1, text: null },
			{ message_id: 2, text: 'part' },
		]);
	});
	it('dedupes the same fragment and the same whole message', () => {
		const q = [
			{ message_id: 1, text: null },
			{ message_id: 2, text: 'x' },
		];
		expect(addQuote(q, { message_id: 1, text: null })).toBe(q);
		expect(addQuote(q, { message_id: 2, text: ' x ' })).toBe(q);
		expect(addQuote(q, { message_id: 1, text: 'part of it' })).toBe(q);
	});
	it('allows different fragments of one message', () => {
		const q = addQuote([{ message_id: 2, text: 'a' }], {
			message_id: 2,
			text: 'b',
		});
		expect(q).toHaveLength(2);
	});
	it('ignores blank fragments', () => {
		const q: never[] = [];
		expect(addQuote(q, { message_id: 1, text: '   ' })).toBe(q);
	});
	it('stops at the limit', () => {
		let q = [] as ReturnType<typeof addQuote>;
		for (let i = 1; i <= MAX_QUOTES + 2; i++)
			q = addQuote(q, { message_id: i, text: null });
		expect(q).toHaveLength(MAX_QUOTES);
	});
	it('clips overlong text', () => {
		const q = addQuote([], {
			message_id: 1,
			text: 'a'.repeat(MAX_QUOTE_TEXT + 10),
		});
		expect(q[0].text).toHaveLength(MAX_QUOTE_TEXT);
	});
});

describe('removeQuote', () => {
	it('removes by index and clears down to empty', () => {
		const q = [
			{ message_id: 1, text: null },
			{ message_id: 2, text: 'b' },
		];
		expect(removeQuote(q, 0)).toEqual([{ message_id: 2, text: 'b' }]);
		expect(removeQuote(removeQuote(q, 0), 0)).toEqual([]);
	});
});

describe('chips', () => {
	const messages = [
		{
			id: 1,
			role: 'assistant' as const,
			content: 'Full   answer\ntext',
			created_at: new Date(2026, 9, 20, 14, 32).toISOString(),
		},
	];
	it('builds a composer chip from the quoted message', () => {
		expect(draftChip({ message_id: 1, text: null }, messages)).toEqual({
			author: 'AI',
			time: '14:32',
			text: 'Full answer text',
		});
		expect(draftChip({ message_id: 1, text: 'bit' }, messages).text).toBe(
			'bit',
		);
	});
	it('falls back when the message is not loaded', () => {
		expect(draftChip({ message_id: 9, text: 'bit' }, messages)).toEqual({
			author: 'Message',
			time: '',
			text: 'bit',
		});
	});
	it('builds a chip from a saved quote', () => {
		expect(
			sentChip({
				message_id: 1,
				role: 'user',
				created_at: new Date(2026, 9, 20, 8, 1).toISOString(),
				text: 'hello',
				whole: false,
			}),
		).toEqual({ author: 'You', time: '08:01', text: 'hello' });
	});
	it('truncates with an ellipsis', () => {
		expect(truncateQuote('a'.repeat(100), 10)).toBe('aaaaaaaaa…');
		expect(truncateQuote('short')).toBe('short');
	});
});

describe('findQuoteSpan', () => {
	it('matches ignoring whitespace differences', () => {
		const hay = 'First paragraph.Second   paragraph here.';
		const span = findQuoteSpan(hay, 'paragraph.\n\nSecond paragraph');
		expect(span).not.toBeNull();
		expect(hay.slice(span!.start, span!.end)).toBe(
			'paragraph.Second   paragraph',
		);
	});
	it('returns offsets into the original string', () => {
		const hay = '  a b  c ';
		const span = findQuoteSpan(hay, 'b c');
		expect(hay.slice(span!.start, span!.end)).toBe('b  c');
	});
	it('is null for missing or blank needles', () => {
		expect(findQuoteSpan('abc', 'xyz')).toBeNull();
		expect(findQuoteSpan('abc', '  \n ')).toBeNull();
	});
});
