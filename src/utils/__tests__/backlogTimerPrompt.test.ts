import { backlogTimerPrompt } from '@/utils/backlogTimerPrompt';

const statuses = [
	{ id: 1, name: 'Backlog', type: 'default' },
	{ id: 2, name: 'In progress', type: 'active' },
	{ id: 3, name: 'Review', type: 'active' },
	{ id: 4, name: 'Done', type: 'completed' },
];

describe('backlogTimerPrompt', () => {
	it('offers the first active status when the task sits in a backlog (default) status', () => {
		expect(backlogTimerPrompt({ title: 'Fix login', status_id: 1 }, statuses)).toEqual({
			targetStatus: statuses[1],
			title: 'Task in Backlog',
			message: 'Task "Fix login" is in backlog. Switch to "In progress" status?',
		});
	});

	it('returns null when the task is already in a non-backlog status', () => {
		expect(backlogTimerPrompt({ title: 'x', status_id: 2 }, statuses)).toBeNull();
		expect(backlogTimerPrompt({ title: 'x', status_id: 4 }, statuses)).toBeNull();
	});

	it('returns null when the workspace has no active status to switch to', () => {
		const noActive = statuses.filter((s) => s.type !== 'active');
		expect(backlogTimerPrompt({ title: 'x', status_id: 1 }, noActive)).toBeNull();
	});

	it('returns null when the task status is unknown or missing', () => {
		expect(backlogTimerPrompt({ title: 'x', status_id: 999 }, statuses)).toBeNull();
		expect(backlogTimerPrompt({ title: 'x', status_id: null }, statuses)).toBeNull();
		expect(backlogTimerPrompt({ title: 'x', status_id: 1 }, undefined)).toBeNull();
	});
});
