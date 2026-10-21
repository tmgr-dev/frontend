import {
	appendPending,
	applyConversation,
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
	nextChatAfterRemoval,
	parseRememberedChatId,
	resetForWorkspace,
	sendErrorMessage,
} from '../agentChat';

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
