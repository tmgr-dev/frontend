import { formatDueTime, formatTimeAgo } from '../relativeTime';

const NOW = Date.parse('2026-09-27T12:00:00Z');

describe('formatTimeAgo', () => {
	it('describes the past', () => {
		expect(formatTimeAgo('2026-09-27T11:55:00Z', NOW)).toBe('5 min ago');
		expect(formatTimeAgo('2026-09-27T09:00:00Z', NOW)).toBe('3 h ago');
	});

	it('describes the future', () => {
		expect(formatTimeAgo('2026-09-27T14:00:00Z', NOW)).toBe('in 2 h');
	});

	it('calls anything under a minute "just now"', () => {
		expect(formatTimeAgo('2026-09-27T11:59:45Z', NOW)).toBe('just now');
	});
});

describe('formatDueTime', () => {
	it('is overdue in the past', () => {
		expect(formatDueTime('2026-09-27T09:00:00Z', NOW)).toEqual({
			text: 'overdue 3 h',
			urgency: 'overdue',
		});
	});

	it('is soon within 24 h', () => {
		expect(formatDueTime('2026-09-27T14:00:00Z', NOW)).toEqual({
			text: 'in 2 h',
			urgency: 'soon',
		});
	});

	it('is later beyond 24 h', () => {
		expect(formatDueTime('2026-09-30T12:00:00Z', NOW)).toEqual({
			text: 'in 3 d',
			urgency: 'later',
		});
	});
});
