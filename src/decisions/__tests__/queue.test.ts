import {
	buildQueue,
	flattenQueue,
	groupKey,
	groupLabel,
	nextCardId,
	queueProgress,
	stepCardId,
	visibleCards,
} from '../queue';
import { makeCard } from './fixtures';

describe('buildQueue', () => {
	it('groups by project directory and labels with its base name', () => {
		const a = makeCard({ id: 'a' });
		const b = makeCard({ id: 'b', created_ms: 2000 });
		const c = makeCard({
			id: 'c',
			source: { agent: 'codex', cwd: '/work/beta/' },
		});
		const groups = buildQueue([a, b, c]);
		expect(groups.map((g) => g.label)).toEqual(['alpha', 'beta']);
		expect(groups[0].cards.map((x) => x.id)).toEqual(['a', 'b']);
	});

	it('falls back to session, then agent', () => {
		const s = makeCard({
			source: { agent: 'codex', session: 's1', title: 'T' },
		});
		const g = makeCard({ source: { agent: 'codex' } });
		expect(groupKey(s)).toBe('session:codex:s1');
		expect(groupLabel(s)).toBe('T');
		expect(groupKey(g)).toBe('agent:codex');
		expect(groupLabel(g)).toBe('codex');
	});

	it('puts blocking cards first, deferred last, then oldest first', () => {
		const older = makeCard({ id: 'older', created_ms: 1 });
		const newer = makeCard({ id: 'newer', created_ms: 5 });
		const blocking = makeCard({
			id: 'blocking',
			created_ms: 9,
			blocking: true,
		});
		const deferred = makeCard({
			id: 'deferred',
			created_ms: 0,
			state: 'deferred',
			blocking: true,
		});
		const ids = flattenQueue(
			buildQueue([deferred, newer, older, blocking]),
		).map((c) => c.id);
		expect(ids).toEqual(['blocking', 'older', 'newer', 'deferred']);
	});

	it('orders groups by their most urgent pending card', () => {
		const calm = makeCard({ id: 'calm', created_ms: 1 });
		const urgent = makeCard({
			id: 'urgent',
			created_ms: 50,
			blocking: true,
			source: { agent: 'codex', cwd: '/work/beta' },
		});
		const groups = buildQueue([calm, urgent]);
		expect(groups.map((g) => g.label)).toEqual(['beta', 'alpha']);
	});

	it('sinks resolved cards below pending ones in a group and groups without pending last', () => {
		const done = makeCard({ id: 'done', state: 'answered', created_ms: 1 });
		const open = makeCard({ id: 'open', created_ms: 2 });
		const other = makeCard({
			id: 'other',
			state: 'answered',
			created_ms: 0,
			source: { agent: 'codex', cwd: '/work/beta' },
		});
		const groups = buildQueue([other, done, open]);
		expect(groups.map((g) => g.label)).toEqual(['alpha', 'beta']);
		expect(groups[0].cards.map((c) => c.id)).toEqual(['open', 'done']);
	});
});

describe('visibleCards and progress', () => {
	it('keeps pending cards and the ones resolved in this window', () => {
		const cards = [
			makeCard({ id: 'p' }),
			makeCard({ id: 'r', state: 'answered' }),
			makeCard({ id: 'old', state: 'closed' }),
		];
		expect(visibleCards(cards, new Set(['r'])).map((c) => c.id)).toEqual([
			'p',
			'r',
		]);
	});

	it('reports done, pending and percent', () => {
		const cards = [
			makeCard({ id: 'a' }),
			makeCard({ id: 'b', state: 'answered' }),
			makeCard({ id: 'c', state: 'dismissed' }),
			makeCard({ id: 'd', state: 'deferred' }),
		];
		expect(queueProgress(cards)).toEqual({
			done: 2,
			total: 4,
			pending: 2,
			percent: 50,
		});
		expect(queueProgress([]).percent).toBe(0);
	});
});

describe('nextCardId and stepCardId', () => {
	const order = [
		makeCard({ id: 'a' }),
		makeCard({ id: 'b', state: 'answered' }),
		makeCard({ id: 'c', state: 'deferred' }),
		makeCard({ id: 'd' }),
	];

	it('advances to the next open card after the current one, wrapping', () => {
		expect(nextCardId(order, 'a')).toBe('d');
		expect(nextCardId(order, 'd')).toBe('a');
	});

	it('prefers open cards over deferred ones and falls back to deferred', () => {
		expect(nextCardId(order, 'a')).toBe('d');
		const onlyDeferred = [
			makeCard({ id: 'a', state: 'answered' }),
			makeCard({ id: 'c', state: 'deferred' }),
		];
		expect(nextCardId(onlyDeferred, 'a')).toBe('c');
	});

	it('returns null when nothing else is pending', () => {
		expect(nextCardId([makeCard({ id: 'a' })], 'a')).toBeNull();
		expect(nextCardId([], null)).toBeNull();
	});

	it('starts from the top when the current card is gone', () => {
		expect(nextCardId(order, 'zzz')).toBe('a');
	});

	it('steps through all cards with wrap-around', () => {
		expect(stepCardId(order, 'd', 1)).toBe('a');
		expect(stepCardId(order, 'a', -1)).toBe('d');
		expect(stepCardId(order, null, 1)).toBe('a');
		expect(stepCardId([], 'a', 1)).toBeNull();
	});
});
