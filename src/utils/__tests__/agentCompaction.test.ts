import type { AgentCompaction, AgentMessage } from '@/types/agent';
import {
	canCompactConversation,
	compactErrorMessage,
	compactionsByAnchor,
	mergeCompaction,
} from '../agentCompaction';

const compaction = (
	id: number,
	up_to_message_id: number,
	trigger: 'manual' | 'auto' = 'manual',
): AgentCompaction => ({
	id,
	up_to_message_id,
	trigger,
	created_at: '2026-10-21T10:00:00Z',
});

const message = (
	id: number,
	status: AgentMessage['status'] = 'done',
): AgentMessage => ({
	id,
	role: id % 2 ? 'user' : 'assistant',
	content: '',
	status,
	steps: [],
	created_at: '',
});

describe('mergeCompaction', () => {
	it('appends a new compaction and keeps the list ordered by id', () => {
		const merged = mergeCompaction([compaction(3, 9)], compaction(2, 5));
		expect(merged.map((c) => c.id)).toEqual([2, 3]);
	});

	it('does not duplicate a compaction already known from history or the POST response', () => {
		const list = [compaction(2, 5)];
		expect(mergeCompaction(list, compaction(2, 5, 'auto'))).toBe(list);
	});
});

describe('compactionsByAnchor', () => {
	it('groups compactions by the message they follow', () => {
		const anchors = compactionsByAnchor([
			compaction(1, 4),
			compaction(2, 8),
			compaction(3, 4, 'auto'),
		]);
		expect(anchors.get(4)?.map((c) => c.id)).toEqual([1, 3]);
		expect(anchors.get(8)?.map((c) => c.id)).toEqual([2]);
		expect(anchors.get(99)).toBeUndefined();
	});
});

describe('canCompactConversation', () => {
	it('needs at least two finished messages', () => {
		expect(canCompactConversation([], false)).toBe(false);
		expect(canCompactConversation([message(1)], false)).toBe(false);
		expect(canCompactConversation([message(1), message(2)], false)).toBe(true);
	});

	it('is blocked while a message is pending or the caller reports it busy', () => {
		const history = [message(1), message(2), message(3, 'pending')];
		expect(canCompactConversation(history, false)).toBe(false);
		expect(canCompactConversation([message(1), message(2)], true)).toBe(false);
	});

	it('ignores failed messages when counting finished ones', () => {
		expect(
			canCompactConversation(
				[message(1), message(2, 'failed'), message(3, 'failed')],
				false,
			),
		).toBe(false);
	});
});

describe('compactErrorMessage', () => {
	it('shows the server message for a 409', () => {
		expect(
			compactErrorMessage(409, 'A turn is still running. Wait for it.'),
		).toBe('A turn is still running. Wait for it.');
	});

	it('falls back when a 409 has no usable message', () => {
		expect(compactErrorMessage(409, '  ')).toBe(
			'Nothing to compact right now.',
		);
	});

	it('explains the rate limit on 429', () => {
		expect(compactErrorMessage(429)).toMatch(/rate limited/);
	});

	it('uses a generic message for anything else', () => {
		expect(compactErrorMessage(500)).toBe(
			'Could not compact the history. Please try again.',
		);
	});
});
