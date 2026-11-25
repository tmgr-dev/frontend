import { hasPollQuestions, pollGroups, pollHidden } from '../pollCards';
import type { ChatMessage, PollWire } from '../types';

const ID = 'poll_abc';

const msg = (id: string, poll: PollWire): ChatMessage => ({
	v: 1,
	id,
	room: { kind: 'manager', machine: 'mx_a' },
	author: { kind: 'manager', id: 'manager' },
	ts_ms: 1,
	kind: 'text',
	body_md: 'x',
	poll,
});

const open = (id: string, cardId = ID, label = 'A'): ChatMessage =>
	msg(id, {
		card_id: cardId,
		session: 's_1',
		questions: [
			{ header: 'h', question: 'q', options: [{ label }, { label: 'B' }] },
		],
	});

const close = (id: string, closed = 'applied', cardId = ID): ChatMessage =>
	msg(id, {
		card_id: cardId,
		session: 's_1',
		closed,
		picked: 'A',
		why_not: 'because',
	});

describe('pollGroups', () => {
	it('groups an open card with its close', () => {
		const g = pollGroups([open('m1'), close('m2')]);
		expect(Object.keys(g)).toEqual(['m1']);
		expect(g.m1.closed).toEqual({
			closed: 'applied',
			picked: 'A',
			why: 'because',
		});
	});

	it('leaves an open card open', () => {
		const g = pollGroups([open('m1')]);
		expect(g.m1.closed).toBeUndefined();
	});

	it('applies a close that arrives before the card', () => {
		const g = pollGroups([close('m1', 'terminal'), open('m2')]);
		expect(g.m2.closed?.closed).toBe('terminal');
	});

	it('ignores duplicate closes and keeps the first', () => {
		const g = pollGroups([
			open('m1'),
			close('m2', 'applied'),
			close('m3', 'gone'),
		]);
		expect(g.m1.closed?.closed).toBe('applied');
	});

	it('does not carry a duplicate late close onto a later repost', () => {
		const g = pollGroups([
			open('m1'),
			close('m2'),
			close('m3', 'gone'),
			open('m4'),
		]);
		expect(g.m4.closed).toBeUndefined();
	});

	it('merges a repost of an open card into the first message', () => {
		const g = pollGroups([open('m1', ID, 'A'), open('m2', ID, 'Z')]);
		expect(Object.keys(g)).toEqual(['m1']);
		expect(g.m1.card.poll.questions?.[0].options[0].label).toBe('Z');
	});

	it('keeps cards with different ids apart', () => {
		const g = pollGroups([
			open('m1', 'poll_a'),
			open('m2', 'poll_b'),
			close('m3', 'gone', 'poll_b'),
		]);
		expect(g.m1.closed).toBeUndefined();
		expect(g.m2.closed?.closed).toBe('gone');
	});

	it('ignores messages without a poll', () => {
		const plain = { ...open('m1'), poll: undefined };
		expect(pollGroups([plain])).toEqual({});
	});
});

describe('pollHidden', () => {
	it('hides closes and non-anchor reposts, shows the anchor and plain messages', () => {
		const list = [open('m1'), open('m2'), close('m3')];
		const g = pollGroups(list);
		expect(list.map((m) => pollHidden(m, g))).toEqual([false, true, true]);
		const plain = { ...open('m9'), poll: undefined };
		expect(pollHidden(plain, g)).toBe(false);
		expect(hasPollQuestions(plain)).toBe(false);
	});

	it('hides a poll message with neither questions nor close', () => {
		const m = msg('m1', { card_id: ID, session: 's_1' });
		expect(pollHidden(m, {})).toBe(true);
	});
});
