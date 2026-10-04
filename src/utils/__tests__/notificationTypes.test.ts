import {
	isPageNotification,
	normalizeNotificationType,
	notificationTarget,
	notificationTitle,
	notificationTypeLabel,
} from '../notificationTypes';

describe('notificationTypeLabel', () => {
	it('labels page types in both spellings', () => {
		expect(notificationTypeLabel('page.created')).toBe('New page');
		expect(notificationTypeLabel('page_updated')).toBe('Page updated');
		expect(notificationTypeLabel('page.mentioned')).toBe(
			'You were mentioned on a page',
		);
	});

	it('keeps existing labels and falls back to the raw type', () => {
		expect(notificationTypeLabel('task_created')).toBe('Task created');
		expect(notificationTypeLabel('weird.type')).toBe('Weird type');
	});

	it('labels agent work and member removal instead of showing the key', () => {
		expect(notificationTypeLabel('member_removed')).toBe('Member removed');
		expect(notificationTypeLabel('agent_work_started')).toBe(
			'Agent work started',
		);
		expect(notificationTypeLabel('agent_work_updated')).toBe(
			'Agent work updated',
		);
	});

	it('normalizes dots', () => {
		expect(normalizeNotificationType('page.mentioned')).toBe('page_mentioned');
		expect(isPageNotification('page.updated')).toBe(true);
		expect(isPageNotification('task_updated')).toBe(false);
	});
});

describe('notificationTarget', () => {
	it('prefers the link, reducing a full url to its path', () => {
		expect(notificationTarget({ link: '/ws/tasks/1' }, 'ws')).toBe(
			'/ws/tasks/1',
		);
		expect(
			notificationTarget({ link: 'https://tmgr.dev/ws/pages/a' }, 'ws'),
		).toBe('/ws/pages/a');
	});

	it('opens the page for a page notification without a link', () => {
		expect(
			notificationTarget(
				{ type: 'page.updated', data: { page_slug: 'notes' } },
				'ws',
			),
		).toBe('/ws/pages/notes');
		expect(
			notificationTarget(
				{ type: 'page.mentioned', data: { page_id: 4 } },
				'ws',
			),
		).toBe('/ws/pages/4');
	});

	it('has no target otherwise', () => {
		expect(notificationTarget({ type: 'task_created' }, 'ws')).toBeNull();
		expect(
			notificationTarget({ type: 'page.created', data: {} }, 'ws'),
		).toBeNull();
	});
});

describe('notificationTitle', () => {
	it('keeps a title the server wrote', () => {
		expect(
			notificationTitle({
				type: 'task_created',
				title: 'Yurij created a task «A»',
			}),
		).toBe('Yurij created a task «A»');
	});

	it('replaces a raw key or empty title with a readable label', () => {
		expect(
			notificationTitle({ type: 'task_created', title: 'task_created' }),
		).toBe('Task created');
		expect(notificationTitle({ type: 'page.created', title: '' })).toBe(
			'New page',
		);
		expect(
			notificationTitle({ type: 'agent_work_started', title: null }),
		).toBe('Agent work started');
	});
});
