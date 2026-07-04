import { api } from './client';

export async function getNotifications(params: Record<string, any> = {}) {
	const { data } = await api.get('notifications', { params });
	return data;
}

export async function getUnreadCount(): Promise<number> {
	const { data } = await api.get('notifications/unread-count');
	return data?.count ?? data?.data?.count ?? 0;
}

export async function markRead(id: number): Promise<void> {
	await api.post(`notifications/${id}/read`);
}

export async function markAllRead(): Promise<void> {
	await api.post('notifications/read-all');
}

export async function getNotificationSettings(): Promise<any> {
	const { data } = await api.get('notification-settings');
	return data?.data ?? data;
}

export async function updateNotificationSettings(payload: any): Promise<any> {
	const { data } = await api.put('notification-settings', payload);
	return data?.data ?? data;
}
