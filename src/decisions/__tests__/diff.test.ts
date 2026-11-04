import {
	awaitsOwner,
	buildAnchors,
	fixMessageText,
	lineNumber,
	splitMessages,
	threadKey,
} from '../diff';
import type { CardMessage, DiffView } from '../types';

const diff: DiffView = {
	truncated: false,
	files: [
		{
			path: 'a.rs',
			added: 1,
			removed: 1,
			hunks: [
				{
					header: '@@ -1,2 +1,2 @@',
					lines: [
						{ kind: 'ctx', old: 1, new: 1, text: 'x' },
						{ kind: 'del', old: 2, text: 'old' },
						{ kind: 'add', new: 2, text: 'new' },
					],
				},
			],
		},
	],
};

const msg = (over: Partial<CardMessage>): CardMessage => ({
	seq: 1,
	author: 'owner',
	text: 't',
	created_ms: 0,
	...over,
});

describe('diff thread keys', () => {
	it('numbers a line by its new number, else the old one', () => {
		expect(lineNumber(diff.files[0].hunks[0].lines[1])).toBe(2);
		expect(lineNumber(diff.files[0].hunks[0].lines[2])).toBe(2);
		expect(threadKey('a.rs', 2)).toBe('a.rs:2');
	});

	it('anchors a thread on the first line with that number', () => {
		const anchors = buildAnchors(diff);
		expect(anchors.get('a.rs:2')).toEqual({
			fileIndex: 0,
			hunkIndex: 0,
			lineIndex: 1,
		});
		expect(anchors.get('a.rs:1')).toEqual({
			fileIndex: 0,
			hunkIndex: 0,
			lineIndex: 0,
		});
		expect(anchors.size).toBe(2);
	});
});

describe('splitMessages', () => {
	it('separates line threads from card chat, ordered by seq', () => {
		const { threads, chat } = splitMessages([
			msg({ seq: 3, author: 'agent', file: 'a.rs', line: 2 }),
			msg({ seq: 1, file: 'a.rs', line: 2 }),
			msg({ seq: 2, text: 'hello' }),
		]);
		expect(chat.map((m) => m.text)).toEqual(['hello']);
		expect(threads.get('a.rs:2')?.map((m) => m.seq)).toEqual([1, 3]);
	});

	it('treats a message with only a file as chat', () => {
		expect(splitMessages([msg({ file: 'a.rs' })]).chat).toHaveLength(1);
	});

	it('knows when the agent spoke last', () => {
		expect(awaitsOwner([msg({}), msg({ author: 'agent' })])).toBe(true);
		expect(awaitsOwner([msg({ author: 'agent' }), msg({})])).toBe(false);
		expect(awaitsOwner([])).toBe(false);
	});
});

describe('fixMessageText', () => {
	it('uses a default text and prefixes the clarification', () => {
		expect(fixMessageText('  ')).toBe('Fix this.');
		expect(fixMessageText(' rename it ')).toBe('Fix: rename it');
	});
});
