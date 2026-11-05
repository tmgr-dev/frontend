import {
	AGENT_WAIT_MS,
	agentWaitNotice,
	awaitsOwner,
	beginRequest,
	buildAnchors,
	diffTotals,
	fixMessageText,
	hasChanges,
	isLatestRequest,
	lineNumber,
	qualifyFile,
	splitMessages,
	threadKey,
} from '../diff';
import type { CardMessage, DiffView } from '../types';
import { makeCard } from './fixtures';

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

describe('diff groups', () => {
	it('qualifies the file only on cards with several groups', () => {
		expect(qualifyFile('api', 'src/a.rs', 1)).toBe('src/a.rs');
		expect(qualifyFile('api', 'src/a.rs', 2)).toBe('api:src/a.rs');
		expect(qualifyFile('ui', 'src/a.rs', 2)).not.toBe(
			qualifyFile('api', 'src/a.rs', 2),
		);
	});

	it('anchors per group under the qualified file', () => {
		const anchors = buildAnchors(diff, 'api');
		expect(anchors.has('api:a.rs:2')).toBe(true);
		expect(anchors.has('a.rs:2')).toBe(false);
	});

	it('detects a card with changes', () => {
		expect(hasChanges(makeCard())).toBe(false);
		expect(hasChanges(makeCard({ diff: { repo: '/r' } }))).toBe(true);
		expect(hasChanges(makeCard({ diffs: [{ repo: '/r' }] }))).toBe(true);
		expect(hasChanges(makeCard({ patches: [{ label: 'p', bytes: 3 }] }))).toBe(
			true,
		);
	});
});

describe('agentWaitNotice', () => {
	const ctx = {
		listening: true,
		sessionGone: false,
		canReply: true,
		final: false,
		now: 10_000,
	};
	const owner = msg({ author: 'owner', created_ms: 10_000 });

	it('shows nothing when there are no messages or the last is the agent', () => {
		expect(agentWaitNotice([], ctx)).toBeNull();
		expect(
			agentWaitNotice([owner, msg({ author: 'agent', seq: 2 })], ctx),
		).toBeNull();
	});

	it('shows nothing for a final card or when replies are impossible', () => {
		expect(agentWaitNotice([owner], { ...ctx, final: true })).toBeNull();
		expect(agentWaitNotice([owner], { ...ctx, canReply: false })).toBeNull();
	});

	it('reports an ended session before anything else', () => {
		expect(
			agentWaitNotice([owner], { ...ctx, sessionGone: true, listening: false }),
		).toBe('The session ended, so no agent will see this.');
	});

	it('reports an ended session over a delivered message', () => {
		const seen = { ...owner, delivered: true };
		expect(
			agentWaitNotice([seen], { ...ctx, sessionGone: true, listening: false }),
		).toBe('The session ended, so no agent will see this.');
	});

	it('reports that nobody is listening while the message is undelivered', () => {
		expect(agentWaitNotice([owner], { ...ctx, listening: false })).toBe(
			"No agent is listening right now; it'll see this when it comes back.",
		);
	});

	it('does not claim nobody has the message once it was delivered', () => {
		const seen = { ...owner, delivered: true };
		expect(agentWaitNotice([seen], { ...ctx, listening: false })).toBe(
			'waiting for the agent…',
		);
	});

	it('waits for a fresh message and times out at exactly three minutes', () => {
		expect(agentWaitNotice([owner], ctx)).toBe('waiting for the agent…');
		const edge = { ...ctx, now: 10_000 + AGENT_WAIT_MS - 1 };
		expect(agentWaitNotice([owner], edge)).toBe('waiting for the agent…');
		const late = { ...ctx, now: 10_000 + AGENT_WAIT_MS };
		expect(agentWaitNotice([owner], late)).toBe(
			"The agent hasn't replied yet; it'll see this when it next checks.",
		);
		expect(agentWaitNotice([{ ...owner, delivered: true }], late)).toBe(
			"The agent hasn't replied yet.",
		);
	});
});

describe('request gate', () => {
	it('lets only the latest request per id write', () => {
		const seqs = {};
		const first = beginRequest(seqs, 'a');
		const second = beginRequest(seqs, 'a');
		const other = beginRequest(seqs, 'b');
		expect(isLatestRequest(seqs, 'a', first)).toBe(false);
		expect(isLatestRequest(seqs, 'a', second)).toBe(true);
		expect(isLatestRequest(seqs, 'b', other)).toBe(true);
	});
});

describe('diffTotals', () => {
	const file = (added: number, removed: number, hunks = 1, binary = false) => ({
		path: 'f',
		added,
		removed,
		binary,
		hunks: Array.from({ length: hunks }, () => ({ header: '@@', lines: [] })),
	});
	const group = (
		files: ReturnType<typeof file>[],
		view: { truncated?: boolean; error?: string } = {},
	) => ({
		key: 'k',
		label: 'l',
		kind: 'repo' as const,
		view: { files, truncated: false, ...view },
	});

	it('sums added and removed lines across all files and groups', () => {
		expect(
			diffTotals([group([file(100, 30), file(5, 1)]), group([file(15, 3)])]),
		).toEqual({ added: 120, removed: 34, exact: true });
	});

	it('is exact and zero for no groups', () => {
		expect(diffTotals([])).toEqual({ added: 0, removed: 0, exact: true });
	});

	it('is not exact when a group is truncated', () => {
		expect(diffTotals([group([file(2, 1)], { truncated: true })])).toEqual({
			added: 2,
			removed: 1,
			exact: false,
		});
	});

	it('is not exact when a group failed to load', () => {
		expect(
			diffTotals([group([file(2, 1)]), group([], { error: 'boom' })]).exact,
		).toBe(false);
	});

	it('is not exact when a file is binary', () => {
		expect(diffTotals([group([file(0, 0, 0, true), file(3, 0)])])).toEqual({
			added: 3,
			removed: 0,
			exact: false,
		});
	});

	it('stays exact for a file with no hunks that is not binary', () => {
		expect(diffTotals([group([file(0, 0, 0)])]).exact).toBe(true);
	});
});
