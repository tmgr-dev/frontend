import {
	toggleReaction,
	normalizeReactions,
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
			[{ emoji: '👍', users: [{ id: 1, name: 'Alice' }, { id: 2, name: 'Bob' }] }],
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
