import type { ChatMember, ChatMessage } from '@/actions/tmgr/chats';
import {
	canStopMessage,
	isPersonaMessage,
	personaMembers,
	personaRefusalKey,
	refusedReplies,
} from '@/utils/chatPersonas';

const msg = (over: Partial<ChatMessage> = {}): ChatMessage => ({
	id: 1,
	chat_id: 1,
	author: { kind: 'persona', id: 9, name: 'Ada' },
	body: null,
	mentions: [],
	parent_message_id: null,
	reply_count: 0,
	last_reply_at: null,
	reactions: [],
	files: [],
	edited_at: null,
	deleted: false,
	created_at: '2026-01-01T00:00:00Z',
	status: 'pending',
	invoked_by: { id: 5, name: 'Me' },
	...over,
});

describe('personaRefusalKey', () => {
	test.each([
		'busy',
		'rate_limited',
		'not_allowed',
		'unavailable',
		'daily_budget',
		'personal_budget',
		'budget_unavailable',
	])('maps %s to its own key', (reason) => {
		expect(personaRefusalKey(reason)).toBe(`chats.persona.refusal.${reason}`);
	});

	test('falls back to unavailable for unknown or missing reasons', () => {
		expect(personaRefusalKey('weird')).toBe(
			'chats.persona.refusal.unavailable',
		);
		expect(personaRefusalKey(undefined)).toBe(
			'chats.persona.refusal.unavailable',
		);
	});
});

describe('canStopMessage', () => {
	test('only the invoker of a pending persona message', () => {
		expect(canStopMessage(msg(), 5)).toBe(true);
		expect(canStopMessage(msg(), 6)).toBe(false);
		expect(canStopMessage(msg(), null)).toBe(false);
	});

	test('not for finished, deleted or human messages', () => {
		expect(canStopMessage(msg({ status: 'done' }), 5)).toBe(false);
		expect(canStopMessage(msg({ status: 'canceled' }), 5)).toBe(false);
		expect(canStopMessage(msg({ deleted: true }), 5)).toBe(false);
		expect(
			canStopMessage(
				msg({ author: { kind: 'user', id: 5, name: 'Me' }, invoked_by: null }),
				5,
			),
		).toBe(false);
	});
});

test('isPersonaMessage checks the author kind', () => {
	expect(isPersonaMessage(msg())).toBe(true);
	expect(
		isPersonaMessage(msg({ author: { kind: 'user', id: 1, name: 'x' } })),
	).toBe(false);
});

test('personaMembers keeps persona members only', () => {
	const members: ChatMember[] = [
		{
			kind: 'user',
			id: 1,
			user_id: 1,
			name: 'A',
			role: 'member',
			joined_at: null,
		},
		{
			kind: 'persona',
			id: 9,
			user_id: null,
			name: 'Ada',
			role: 'member',
			joined_at: null,
		},
	];
	expect(personaMembers(members).map((m) => m.id)).toEqual([9]);
});

test('refusedReplies drops queued replies', () => {
	expect(
		refusedReplies([
			{ persona_id: 1, status: 'queued', message_id: 3 },
			{ persona_id: 2, status: 'refused', reason: 'busy' },
		]).map((r) => r.persona_id),
	).toEqual([2]);
	expect(refusedReplies(undefined)).toEqual([]);
});
