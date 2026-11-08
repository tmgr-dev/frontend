import { t } from '@/i18n';

const NOTIFICATION_TYPES = [
	'task_created',
	'task_updated',
	'task_status_changed',
	'task_assigned',
	'task_deleted',
	'task_restored',
	'task_completed',
	'comment_created',
	'comment_updated',
	'comment_deleted',
	'category_created',
	'category_updated',
	'category_deleted',
	'category_restored',
	'file_uploaded',
	'file_deleted',
	'member_joined',
	'member_left',
	'member_removed',
	'task_persona_assigned',
	'task_persona_unassigned',
	'comment_reaction_toggled',
	'agent_work_started',
	'agent_work_updated',
	'agent_work_finished',
	'page_created',
	'page_updated',
	'page_mentioned',
	'memory_added',
];

const camelType = (type: string): string =>
	type.replace(/_([a-z])/g, (_, char: string) => char.toUpperCase());

export const normalizeNotificationType = (type: string): string =>
	(type || '').replace(/\./g, '_');

const humanizeType = (type: string): string => {
	const words = normalizeNotificationType(type).replace(/[_-]+/g, ' ').trim();
	return words ? words.charAt(0).toUpperCase() + words.slice(1) : '';
};

export const notificationTypeLabel = (type: string): string =>
	NOTIFICATION_TYPES.includes(normalizeNotificationType(type))
		? t(
				`workspace.helpers.notificationTypes.${camelType(normalizeNotificationType(type))}`,
			)
		: humanizeType(type) || type;

export const notificationTitle = (notification: {
	type?: string;
	title?: string | null;
}): string => {
	const title = (notification.title ?? '').trim();
	if (title && title !== notification.type) return title;
	return (
		notificationTypeLabel(notification.type ?? '') ||
		t('workspace.helpers.notificationTypes.fallback')
	);
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
