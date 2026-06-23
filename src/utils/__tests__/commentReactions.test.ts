import {
	toggleReaction,
	normalizeReactions,
	mergeServerReactionForEmoji,
	extractReactionsPayload,
	ReactionSummary,
} from '../commentReactions';

const ALICE = { id: 1, name: 'Alice' };
const BOB = { id: 2, name: 'Bob' };

describe('toggleReaction', () => {
	it('adds a brand new emoji the user has not reacted with', () => {
		const result = toggleReaction([], '👍', ALICE);
		expect(result).toEqual([
			{ emoji: '👍', count: 1, reacted: true, users: [ALICE] },
		]);
	});

	it('increments an existing emoji the user has not yet reacted with', () => {
		const start: ReactionSummary[] = [
			{ emoji: '👍', count: 1, reacted: false, users: [BOB] },
		];
		const result = toggleReaction(start, '👍', ALICE);
		expect(result[0].count).toBe(2);
		expect(result[0].reacted).toBe(true);
		expect(result[0].users).toEqual([BOB, ALICE]);
	});

	it('removes the user and decrements when toggling off', () => {
		const start: ReactionSummary[] = [
			{ emoji: '👍', count: 2, reacted: true, users: [BOB, ALICE] },
		];
		const result = toggleReaction(start, '👍', ALICE);
		expect(result[0].count).toBe(1);
		expect(result[0].reacted).toBe(false);
		expect(result[0].users).toEqual([BOB]);
	});

	it('drops the emoji entirely when the last reaction is removed', () => {
		const start: ReactionSummary[] = [
			{ emoji: '🎉', count: 1, reacted: true, users: [ALICE] },
		];
		expect(toggleReaction(start, '🎉', ALICE)).toEqual([]);
	});

	it('does not mutate the input array or its entries', () => {
		const start: ReactionSummary[] = [
			{ emoji: '👍', count: 1, reacted: false, users: [BOB] },
		];
		const snapshot = JSON.parse(JSON.stringify(start));
		toggleReaction(start, '👍', ALICE);
		expect(start).toEqual(snapshot);
	});
});

describe('normalizeReactions', () => {
	it('returns an empty array for non-array input', () => {
		expect(normalizeReactions(undefined)).toEqual([]);
		expect(normalizeReactions(null)).toEqual([]);
		expect(normalizeReactions({})).toEqual([]);
	});

	it('derives count and reacted from users when not provided', () => {
		const result = normalizeReactions(
			[
				{
					emoji: '👍',
					users: [
						{ id: 1, name: 'Alice' },
						{ id: 2, name: 'Bob' },
					],
				},
			],
			1,
		);
		expect(result).toEqual([
			{ emoji: '👍', count: 2, reacted: true, users: [ALICE, BOB] },
		]);
	});

	it('honours explicit count and reacted fields from the API', () => {
		const result = normalizeReactions([
			{ emoji: '🚀', count: 5, reacted: false },
		]);
		expect(result).toEqual([
			{ emoji: '🚀', count: 5, reacted: false, users: [] },
		]);
	});

	it('skips malformed entries and zero-count reactions', () => {
		const result = normalizeReactions([
			{ emoji: '👍', count: 0, users: [] },
			{ noEmoji: true },
			null,
			{ emoji: '❤️', count: 1, reacted: true },
		]);
		expect(result).toEqual([
			{ emoji: '❤️', count: 1, reacted: true, users: [] },
		]);
	});
});

describe('extractReactionsPayload (API response shapes)', () => {
	const arr = [{ emoji: '👍', count: 1, reacted: true }];

	it('returns a raw array as-is (Java-style response)', () => {
		expect(extractReactionsPayload(arr)).toBe(arr);
	});

	it('unwraps a { data } wrapper', () => {
		expect(extractReactionsPayload({ data: arr })).toBe(arr);
	});

	it('unwraps a { reactions } wrapper', () => {
		expect(extractReactionsPayload({ reactions: arr })).toBe(arr);
	});

	it('falls back to [] for null/undefined/objects without a known key', () => {
		expect(extractReactionsPayload(null)).toEqual([]);
		expect(extractReactionsPayload(undefined)).toEqual([]);
		expect(extractReactionsPayload({})).toEqual([]);
	});

	it('round-trips through normalizeReactions for both shapes', () => {
		const raw = [{ emoji: '🚀', count: 2, reacted: false }];
		expect(normalizeReactions(extractReactionsPayload(raw))).toEqual([
			{ emoji: '🚀', count: 2, reacted: false, users: [] },
		]);
		expect(normalizeReactions(extractReactionsPayload({ data: raw }))).toEqual([
			{ emoji: '🚀', count: 2, reacted: false, users: [] },
		]);
	});
});

describe('mergeServerReactionForEmoji (concurrent out-of-order toggle race)', () => {
	it('applies a slow emoji response WITHOUT clobbering a concurrently-toggled different emoji', () => {
		// User toggled 👍 then ❤️ quickly; both are optimistically present.
		const current: ReactionSummary[] = [
			{ emoji: '👍', count: 1, reacted: true, users: [ALICE] },
			{ emoji: '❤️', count: 1, reacted: true, users: [ALICE] },
		];
		// The slower 👍 response lands; server's snapshot at that time only knew 👍.
		const serverThumbsResponse: ReactionSummary[] = [
			{ emoji: '👍', count: 1, reacted: true, users: [ALICE] },
		];

		const merged = mergeServerReactionForEmoji(
			current,
			serverThumbsResponse,
			'👍',
		);

		// ❤️ must survive — the old wholesale-replace dropped it (the P2).
		expect(merged).toEqual([
			{ emoji: '❤️', count: 1, reacted: true, users: [ALICE] },
			{ emoji: '👍', count: 1, reacted: true, users: [ALICE] },
		]);
	});

	it('removes only the target emoji when the server reports it gone, keeping others', () => {
		const current: ReactionSummary[] = [
			{ emoji: '👍', count: 1, reacted: true, users: [ALICE] },
			{ emoji: '❤️', count: 1, reacted: true, users: [ALICE] },
		];
		// Server response for 👍 toggle-off: 👍 absent (or zero-count).
		const merged = mergeServerReactionForEmoji(current, [], '👍');
		expect(merged).toEqual([
			{ emoji: '❤️', count: 1, reacted: true, users: [ALICE] },
		]);
	});

	it('updates the target emoji to the authoritative server count/users', () => {
		const current: ReactionSummary[] = [
			{ emoji: '👍', count: 1, reacted: true, users: [ALICE] },
		];
		const server: ReactionSummary[] = [
			{ emoji: '👍', count: 2, reacted: true, users: [ALICE, BOB] },
		];
		expect(mergeServerReactionForEmoji(current, server, '👍')).toEqual([
			{ emoji: '👍', count: 2, reacted: true, users: [ALICE, BOB] },
		]);
	});

	it('does not mutate the input arrays or their entries', () => {
		const current: ReactionSummary[] = [
			{ emoji: '👍', count: 1, reacted: true, users: [ALICE] },
			{ emoji: '❤️', count: 1, reacted: true, users: [ALICE] },
		];
		const snapshot = JSON.parse(JSON.stringify(current));
		mergeServerReactionForEmoji(current, [], '👍');
		expect(current).toEqual(snapshot);
	});
});
