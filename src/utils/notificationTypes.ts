export const NOTIFICATION_TYPE_LABELS: Record<string, string> = {
	task_created: 'Task created',
	task_updated: 'Task updated',
	task_status_changed: 'Task status changed',
	task_assigned: 'Task assigned',
	task_deleted: 'Task deleted',
	task_restored: 'Task restored',
	task_completed: 'Task completed',
	comment_created: 'New comment',
	comment_updated: 'Comment updated',
	comment_deleted: 'Comment deleted',
	category_created: 'Category created',
	category_updated: 'Category updated',
	category_deleted: 'Category deleted',
	category_restored: 'Category restored',
	file_uploaded: 'File uploaded',
	file_deleted: 'File deleted',
	member_joined: 'Member joined',
	member_left: 'Member left workspace',
	member_removed: 'Member removed',
	task_persona_assigned: 'Task assigned to a persona',
	task_persona_unassigned: 'Task unassigned from a persona',
	comment_reaction_toggled: 'Comment reaction',
	agent_work_started: 'Agent work started',
	agent_work_updated: 'Agent work updated',
	agent_work_finished: 'Agent work finished',
	page_created: 'New page',
	page_updated: 'Page updated',
	page_mentioned: 'You were mentioned on a page',
};

export const normalizeNotificationType = (type: string): string =>
	(type || '').replace(/\./g, '_');

const humanizeType = (type: string): string => {
	const words = normalizeNotificationType(type)
		.replace(/[_-]+/g, ' ')
		.trim();
	return words ? words.charAt(0).toUpperCase() + words.slice(1) : '';
};

export const notificationTypeLabel = (type: string): string =>
	NOTIFICATION_TYPE_LABELS[normalizeNotificationType(type)] ??
	(humanizeType(type) || type);

export const notificationTitle = (notification: {
	type?: string;
	title?: string | null;
}): string => {
	const title = (notification.title ?? '').trim();
	if (title && title !== notification.type) return title;
	return notificationTypeLabel(notification.type ?? '') || 'Notification';
};

export const isPageNotification = (type: string): boolean =>
	normalizeNotificationType(type).startsWith('page_');

interface NotificationLike {
	type?: string;
	link?: string | null;
	data?: Record<string, any> | null;
}

export const notificationTarget = (
	notification: NotificationLike,
	workspaceCode: string,
): string | null => {
	const link = notification.link;
	if (link) {
		if (/^https?:\/\//.test(link)) {
			try {
				return new URL(link).pathname;
			} catch {
				return link;
			}
		}
		return link;
	}
	if (!isPageNotification(notification.type ?? '')) return null;
	const data = notification.data ?? {};
	const target = data.page_slug ?? data.slug ?? data.page_id;
	if (!target || !workspaceCode) return null;
	return `/${workspaceCode}/pages/${encodeURIComponent(String(target))}`;
};
