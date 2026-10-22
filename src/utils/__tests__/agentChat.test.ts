import type { PendingAction } from '@/types/agent';
import {
	appendPending,
	applyConfirmation,
	applyConversation,
	applyDecision,
	applyReply,
	applyStep,
	chatTitle,
	createAgentChatState,
	formatChatTime,
	hasMessage,
	hasPendingLast,
	isBusy,
	isPollResultCurrent,
	lastChatKey,
	mergePendingActions,
	nextChatAfterRemoval,
	parseRememberedChatId,
	resetForWorkspace,
	sendErrorMessage,
} from '../agentChat';
import { openActionIds } from '../agentConfirmation';

const CONV = {
	id: 5,
	workspace_id: 114,
	task_id: null,
	created_at: '2026-09-09T10:00:00Z',
};
const USER_MSG = {
	id: 10,
	content: 'what is overdue?',
	created_at: '2026-09-09T10:00:01Z',
};

describe('applyConversation', () => {
	it('stores the conversation and its messages and clears any pending', () => {
		const s = applyConversation(createAgentChatState(), CONV, [
			{
				id: 1,
				role: 'user',
				content: 'hi',
				status: 'done',
				steps: [],
				created_at: '',
			},
		]);
		expect(s.conversationId).toBe(5);
		expect(s.workspaceId).toBe(114);
		expect(s.messages).toHaveLength(1);
		expect(s.pendingId).toBeNull();
	});
	it('keeps the identity of messages that did not change on a reload', () => {
		const message = (id: number, content: string) => ({
			id,
			role: 'user' as const,
			content,
			status: 'done' as const,
			steps: [],
			created_at: '',
		});
		const first = applyConversation(createAgentChatState(), CONV, [
			message(1, 'one'),
			message(2, 'two'),
		]);
		const next = applyConversation(first, CONV, [
			message(1, 'one'),
			message(2, 'two edited'),
			message(3, 'three'),
		]);
		expect(next.messages[0]).toBe(first.messages[0]);
		expect(next.messages[1]).not.toBe(first.messages[1]);
		expect(next.messages[1].content).toBe('two edited');
		expect(next.messages).toHaveLength(3);
	});
	it('marks a still-pending assistant message from history as pending', () => {
		const s = applyConversation(createAgentChatState(), CONV, [
			{
				id: 11,
				role: 'assistant',
				content: '',
				status: 'pending',
				steps: [],
				created_at: '',
			},
		]);
		expect(s.pendingId).toBe(11);
		expect(isBusy(s)).toBe(true);
	});
});

describe('appendPending', () => {
	it('adds the user message and an empty pending assistant message', () => {
		const s = appendPending(
			applyConversation(createAgentChatState(), CONV, []),
			USER_MSG,
			11,
		);
		expect(s.messages.map((m) => [m.id, m.role, m.status])).toEqual([
			[10, 'user', 'done'],
			[11, 'assistant', 'pending'],
		]);
		expect(s.pendingId).toBe(11);
	});
});

describe('applyStep', () => {
	const base = appendPending(
		applyConversation(createAgentChatState(), CONV, []),
		USER_MSG,
		11,
	);
	it('appends steps to the pending message in seq order and dedupes redeliveries', () => {
		let s = applyStep(base, {
			conversation_id: 5,
			message_id: 11,
			task_id: null,
			seq: 2,
			tool: 'get_task',
			summary: 'task_id=9',
		});
		s = applyStep(s, {
			conversation_id: 5,
			message_id: 11,
			task_id: null,
			seq: 1,
			tool: 'deadline_report',
			summary: '',
		});
		s = applyStep(s, {
			conversation_id: 5,
			message_id: 11,
			task_id: null,
			seq: 1,
			tool: 'deadline_report',
			summary: '',
		});
		expect(s.messages[1].steps.map((x) => x.seq)).toEqual([1, 2]);
	});
	it('ignores steps of another conversation', () => {
		const s = applyStep(base, {
			conversation_id: 6,
			message_id: 99,
			task_id: null,
			seq: 1,
			tool: 'x',
			summary: '',
		});
		expect(s).toBe(base);
	});
});

describe('applyReply', () => {
	const base = appendPending(
		applyConversation(createAgentChatState(), CONV, []),
		USER_MSG,
		11,
	);
	it('fills the pending message and clears pending', () => {
		const s = applyReply(base, {
			conversation_id: 5,
			message_id: 11,
			task_id: null,
			status: 'done',
			content: 'TM-1 is overdue',
			steps: [{ seq: 1, tool: 'deadline_report', summary: '' }],
		});
		expect(s.messages[1]).toMatchObject({
			status: 'done',
			content: 'TM-1 is overdue',
		});
		expect(s.messages[1].steps).toHaveLength(1);
		expect(s.pendingId).toBeNull();
	});
	it('keeps a failed reply visible as failed', () => {
		const s = applyReply(base, {
			conversation_id: 5,
			message_id: 11,
			task_id: null,
			status: 'failed',
			content: 'unavailable',
			steps: [],
		});
		expect(s.messages[1].status).toBe('failed');
		expect(isBusy(s)).toBe(false);
	});
	it('appends the reply when the pending message is unknown (event arrived before the 202)', () => {
		const s = applyReply(applyConversation(createAgentChatState(), CONV, []), {
			conversation_id: 5,
			message_id: 12,
			task_id: null,
			status: 'done',
			content: 'late',
			steps: [],
		});
		expect(s.messages.map((m) => m.id)).toEqual([12]);
	});
	it('ignores a reply for another conversation', () => {
		const s = applyReply(base, {
			conversation_id: 6,
			message_id: 99,
			task_id: null,
			status: 'done',
			content: 'x',
			steps: [],
		});
		expect(s).toBe(base);
	});
	it('keeps accumulated steps when the reply carries none', () => {
		let s = applyStep(base, {
			conversation_id: 5,
			message_id: 11,
			task_id: null,
			seq: 1,
			tool: 'get_task',
			summary: '',
		});
		s = applyStep(s, {
			conversation_id: 5,
			message_id: 11,
			task_id: null,
			seq: 2,
			tool: 'deadline_report',
			summary: '',
		});
		s = applyReply(s, {
			conversation_id: 5,
			message_id: 11,
			task_id: null,
			status: 'done',
			content: 'done',
			steps: [],
		});
		expect(s.messages[1].steps).toHaveLength(2);
	});
});

describe('hasMessage', () => {
	const base = appendPending(
		applyConversation(createAgentChatState(), CONV, []),
		USER_MSG,
		11,
	);
	it('returns true for a message id already in state', () => {
		expect(hasMessage(base, 11)).toBe(true);
	});
	it('returns false for an unknown message id', () => {
		expect(hasMessage(base, 999)).toBe(false);
	});
});

describe('resetForWorkspace', () => {
	const s = applyConversation(createAgentChatState(), CONV, []);
	it('returns the same state for the same workspace', () =>
		expect(resetForWorkspace(s, 114)).toBe(s));
	it('returns an empty state for another workspace', () =>
		expect(resetForWorkspace(s, 56)).toEqual(createAgentChatState()));
});

describe('sendErrorMessage', () => {
	it('maps 413 to a too-long message', () => {
		expect(sendErrorMessage(413)).toBe('Your question is too long.');
	});
	it('maps 422 to a blank-question message', () => {
		expect(sendErrorMessage(422)).toBe('Type a question first.');
	});
	it('maps 429 to a still-answering message', () => {
		expect(sendErrorMessage(429)).toBe(
			'The assistant is still answering your previous question.',
		);
	});
	it('maps anything else (including no status) to a generic retry message', () => {
		expect(sendErrorMessage(500)).toBe(
			'Could not send your question. Please try again.',
		);
		expect(sendErrorMessage(undefined)).toBe(
			'Could not send your question. Please try again.',
		);
	});
});

describe('chat list helpers', () => {
	const msg = (status: 'pending' | 'done' | 'failed') => ({
		id: 1,
		role: 'assistant' as const,
		content: '',
		status,
		steps: [],
		created_at: '2026-09-09T10:00:00Z',
	});

	it('builds the remembered-chat key per user and workspace', () => {
		expect(lastChatKey(3, 114)).toBe('tmgr.agentChat.last.3.114');
	});

	it('parses a remembered chat id and rejects junk', () => {
		expect(parseRememberedChatId('42')).toBe(42);
		expect(parseRememberedChatId(null)).toBeNull();
		expect(parseRememberedChatId('abc')).toBeNull();
		expect(parseRememberedChatId('0')).toBeNull();
		expect(parseRememberedChatId('-5')).toBeNull();
	});

	it('detects a pending last message', () => {
		expect(hasPendingLast([])).toBe(false);
		expect(hasPendingLast([msg('done')])).toBe(false);
		expect(hasPendingLast([msg('done'), msg('pending')])).toBe(true);
	});

	it('picks the neighbour after removal, or null when nothing is left', () => {
		const items = [{ id: 1 }, { id: 2 }, { id: 3 }];
		expect(nextChatAfterRemoval(items, 2)).toBe(3);
		expect(nextChatAfterRemoval(items, 3)).toBe(2);
		expect(nextChatAfterRemoval([{ id: 1 }], 1)).toBeNull();
	});

	it('accepts a poll result only for the same pending message and conversation', () => {
		const s = appendPending(
			applyConversation(createAgentChatState(), CONV, []),
			USER_MSG,
			11,
		);
		expect(isPollResultCurrent(s, 5, 11)).toBe(true);
		expect(isPollResultCurrent(s, 6, 11)).toBe(false);
		expect(isPollResultCurrent(s, 5, 12)).toBe(false);
		expect(isPollResultCurrent(s, 5, null)).toBe(false);
		const answered = applyReply(s, {
			conversation_id: 5,
			message_id: 11,
			content: 'ok',
			status: 'done',
			steps: [],
		} as never);
		expect(isPollResultCurrent(answered, 5, 11)).toBe(false);
	});

	it('falls back to New chat for empty titles', () => {
		expect(chatTitle(null)).toBe('New chat');
		expect(chatTitle('  ')).toBe('New chat');
		expect(chatTitle('Overdue')).toBe('Overdue');
	});

	it('formats relative and absolute chat times', () => {
		const now = Date.parse('2026-10-14T12:00:00Z');
		expect(formatChatTime(null, now)).toBe('');
		expect(formatChatTime('2026-10-14T11:59:30Z', now)).toBe('just now');
		expect(formatChatTime('2026-10-14T11:30:00Z', now)).toBe('30 min ago');
		expect(formatChatTime('2026-10-14T10:00:00Z', now)).toBe('2 h ago');
		expect(formatChatTime('2026-10-01T12:00:00Z', now)).toBe('Oct 1');
		expect(formatChatTime('2025-10-01T12:00:00Z', now)).toBe('Oct 1, 2025');
	});
});

describe('confirmation flow', () => {
	const action = (id: number, status = 'pending') =>
		({
			id,
			tool: 't',
			risk: 'low',
			status,
			preview: { title: 'x', fields: [], body: null },
			result: null,
			expires_at: '2026-10-21T10:15:00Z',
		} as unknown as PendingAction);

	const withPending = () =>
		appendPending(
			applyConversation(createAgentChatState(), CONV, []),
			USER_MSG,
			11,
		);

	it('moves a pending message to awaiting and frees the input', () => {
		const s = applyConfirmation(withPending(), {
			conversation_id: 5,
			message_id: 11,
			actions: [{ ...action(1), risk: 'HIGH' } as never],
		});
		const m = s.messages.find((x) => x.id === 11);
		expect(m?.status).toBe('awaiting');
		expect(m?.pending_actions?.[0].risk).toBe('high');
		expect(s.pendingId).toBeNull();
		expect(isBusy(s)).toBe(false);
	});

	it('ignores other conversations and unknown messages', () => {
		const base = withPending();
		expect(
			applyConfirmation(base, {
				conversation_id: 99,
				message_id: 11,
				actions: [action(1)],
			}),
		).toBe(base);
		expect(
			applyConfirmation(base, {
				conversation_id: 5,
				message_id: 77,
				actions: [action(1)],
			}),
		).toBe(base);
	});

	it('updates statuses without leaving awaiting while nothing is decided', () => {
		let s = applyConfirmation(withPending(), {
			conversation_id: 5,
			message_id: 11,
			actions: [action(1)],
		});
		s = applyConfirmation(s, {
			conversation_id: 5,
			message_id: 11,
			actions: [action(1, 'superseded')],
		});
		const m = s.messages.find((x) => x.id === 11);
		expect(m?.pending_actions?.[0].status).toBe('superseded');
		expect(m?.status).toBe('awaiting');
	});

	it('re-pauses a running message on a new card', () => {
		let s = applyConfirmation(withPending(), {
			conversation_id: 5,
			message_id: 11,
			actions: [action(1)],
		});
		s = applyDecision(s, 11, [action(1, 'executing')]);
		s = applyConfirmation(s, {
			conversation_id: 5,
			message_id: 11,
			actions: [action(2)],
		});
		expect(s.messages.find((x) => x.id === 11)?.status).toBe('awaiting');
		expect(s.pendingId).toBeNull();
	});

	it('marks awaiting as running after a decision and makes the chat busy', () => {
		let s = applyConfirmation(withPending(), {
			conversation_id: 5,
			message_id: 11,
			actions: [action(1)],
		});
		s = applyDecision(s, 11, [action(1, 'executed')]);
		const m = s.messages.find((x) => x.id === 11);
		expect(m?.status).toBe('running');
		expect(m?.pending_actions?.[0].status).toBe('executed');
		expect(s.pendingId).toBe(11);
		expect(isBusy(s)).toBe(true);
	});

	it('leaves a message that already finished alone on a late decision', () => {
		let s = applyReply(withPending(), {
			conversation_id: 5,
			message_id: 11,
			status: 'done',
			content: 'ok',
			steps: [],
		});
		s = applyDecision(s, 11, [action(1, 'executed')]);
		const m = s.messages.find((x) => x.id === 11);
		expect(m?.status).toBe('done');
		expect(s.pendingId).toBeNull();
	});

	it('treats running as busy and awaiting as idle when loading history', () => {
		const base = {
			role: 'assistant' as const,
			content: '',
			steps: [],
			created_at: '',
		};
		const running = applyConversation(createAgentChatState(), CONV, [
			{ ...base, id: 1, status: 'running' },
		]);
		const awaiting = applyConversation(createAgentChatState(), CONV, [
			{ ...base, id: 1, status: 'awaiting' },
		]);
		expect(isBusy(running)).toBe(true);
		expect(isBusy(awaiting)).toBe(false);
		expect(hasPendingLast(running.messages)).toBe(true);
		expect(hasPendingLast(awaiting.messages)).toBe(false);
	});

	it('keeps a newer card when a late decision response arrives', () => {
		let s = applyConfirmation(withPending(), {
			conversation_id: 5,
			message_id: 11,
			actions: [action(1)],
		});
		s = applyDecision(s, 11, [action(1, 'executed')]);
		s = applyConfirmation(s, {
			conversation_id: 5,
			message_id: 11,
			actions: [action(1, 'executed'), action(2)],
		});
		s = applyDecision(s, 11, [action(1, 'executed')]);
		const m = s.messages.find((x) => x.id === 11);
		expect(m?.pending_actions?.map((a) => [a.id, a.status])).toEqual([
			[1, 'executed'],
			[2, 'pending'],
		]);
		expect(m?.status).toBe('awaiting');
		expect(s.pendingId).toBeNull();
	});

	it('keeps a decided card decided on a stale pending event', () => {
		let s = applyConfirmation(withPending(), {
			conversation_id: 5,
			message_id: 11,
			actions: [action(1)],
		});
		s = applyDecision(s, 11, [action(1, 'executed')]);
		s = applyConfirmation(s, {
			conversation_id: 5,
			message_id: 11,
			actions: [action(1)],
		});
		const m = s.messages.find((x) => x.id === 11);
		expect(m?.pending_actions?.[0].status).toBe('executed');
		expect(openActionIds(m?.pending_actions ?? [])).toEqual([]);
		expect(m?.status).toBe('running');
	});

	it('appends new action ids', () => {
		let s = applyConfirmation(withPending(), {
			conversation_id: 5,
			message_id: 11,
			actions: [action(1)],
		});
		s = applyConfirmation(s, {
			conversation_id: 5,
			message_id: 11,
			actions: [action(1), action(2)],
		});
		const m = s.messages.find((x) => x.id === 11);
		expect(m?.pending_actions?.map((a) => a.id)).toEqual([1, 2]);
	});
});

describe('mergePendingActions', () => {
	const a = (id: number, status: string, title = 'x') =>
		({
			id,
			tool: 't',
			risk: 'low',
			status,
			preview: { title, fields: [], body: null },
			result: null,
			expires_at: '',
		} as never);

	it('never moves a status backwards', () => {
		const merged = mergePendingActions(
			[a(1, 'executing'), a(2, 'executed'), a(3, 'pending')],
			[a(1, 'pending'), a(2, 'executing'), a(3, 'executing')],
		);
		expect(merged.map((x) => x.status)).toEqual([
			'executing',
			'executed',
			'executing',
		]);
	});

	it('takes the other fields from the winning side', () => {
		const merged = mergePendingActions(
			[a(1, 'pending', 'old')],
			[a(1, 'executed', 'new')],
		);
		expect(merged[0].preview.title).toBe('new');
	});

	it('keeps the current terminal status over a different terminal one', () => {
		const merged = mergePendingActions([a(1, 'cancelled')], [a(1, 'executed')]);
		expect(merged[0].status).toBe('cancelled');
	});

	it('keeps ids present only in the current state', () => {
		const merged = mergePendingActions(
			[a(1, 'pending'), a(2, 'pending')],
			[a(1, 'pending')],
		);
		expect(merged.map((x) => x.id)).toEqual([1, 2]);
	});
});

describe('applyReply stats', () => {
	const base = () =>
		applyConversation(createAgentChatState(), CONV, [
			{
				id: 11,
				role: 'assistant',
				content: '',
				status: 'running',
				steps: [],
				created_at: '2026-09-09T10:00:02Z',
			},
		]);
	const event = {
		conversation_id: 5,
		message_id: 11,
		status: 'done' as const,
		content: 'ok',
		steps: [],
	};

	it('merges duration and tokens into an existing message', () => {
		const s = applyReply(base(), {
			...event,
			duration_ms: 1200,
			input_tokens: 30,
			output_tokens: 4,
		});
		expect(s.messages[0]).toMatchObject({
			duration_ms: 1200,
			input_tokens: 30,
			output_tokens: 4,
		});
	});
	it('keeps earlier stats when the event carries none', () => {
		const s0 = applyReply(base(), { ...event, duration_ms: 500 });
		const s1 = applyReply(s0, event);
		expect(s1.messages[0].duration_ms).toBe(500);
	});
	it('stores nulls from the event', () => {
		const s = applyReply(base(), { ...event, input_tokens: null });
		expect(s.messages[0].input_tokens).toBeNull();
	});
	it('adds stats to a reply for an unknown message', () => {
		const s = applyReply(createAgentChatState(), { ...event, duration_ms: 9 });
		expect(s.messages).toHaveLength(0);
		const withConv = applyConversation(createAgentChatState(), CONV, []);
		expect(
			applyReply(withConv, { ...event, duration_ms: 9 }).messages[0],
		).toMatchObject({
			duration_ms: 9,
		});
	});
});

describe('appendPending quotes', () => {
	const quote = {
		message_id: 3,
		role: 'assistant' as const,
		created_at: '2026-09-09T10:00:00Z',
		text: 'bit',
		whole: false,
	};
	it('puts the saved quotes on the optimistic user message', () => {
		const s = appendPending(
			applyConversation(createAgentChatState(), CONV, []),
			{ ...USER_MSG, quotes: [quote] },
			11,
		);
		expect(s.messages[0].quotes).toEqual([quote]);
	});
	it('omits the field when there are none', () => {
		const s = appendPending(createAgentChatState(), USER_MSG, 11);
		expect(s.messages[0]).not.toHaveProperty('quotes');
	});
});
