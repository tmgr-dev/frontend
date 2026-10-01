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
	page_created: 'Страница создана',
	page_updated: 'Страница изменена',
	page_mentioned: 'Вас упомянули на странице',
};

export const normalizeNotificationType = (type: string): string =>
	(type || '').replace(/\./g, '_');

export const notificationTypeLabel = (type: string): string =>
	NOTIFICATION_TYPE_LABELS[normalizeNotificationType(type)] ?? type;

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
