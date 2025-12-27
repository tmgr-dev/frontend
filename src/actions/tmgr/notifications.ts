import $axios from '@/plugins/axios';

export interface Notification {
	id: number;
	user_id: number;
	workspace_id: number;
	activity_id: number | null;
	type: string;
	title: string;
	message: string | null;
	data: Record<string, any> | null;
	link: string | null;
	read_at: string | null;
	created_at: string;
	updated_at: string;
	workspace?: {
		id: number;
		name: string;
		code: string;
	};
}

export interface NotificationPaginatedResponse {
	data: Notification[];
	current_page: number;
	last_page: number;
	per_page: number;
	total: number;
}

export interface NotificationSettings {
	id: number;
	user_id: number;
	workspace_id: number;
	telegram_enabled: boolean;
	telegram_types: string[] | null;
	web_enabled: boolean;
	web_types: string[] | null;
	email_enabled: boolean;
	email_types: string[] | null;
	created_at: string;
	updated_at: string;
}

export interface NotificationSettingsResponse {
	settings: NotificationSettings;
	available_types: string[];
}

export interface UpdateNotificationSettingsPayload {
	telegram_enabled?: boolean;
	telegram_types?: string[];
	web_enabled?: boolean;
	web_types?: string[];
	email_enabled?: boolean;
	email_types?: string[];
}

export const getNotifications = async (
	page: number = 1,
	perPage: number = 20
): Promise<NotificationPaginatedResponse> => {
	const response = await $axios.get('/notifications', {
		params: { page, per_page: perPage },
	});
	return response.data;
};

export const getUnreadCount = async (): Promise<number> => {
	const response = await $axios.get('/notifications/unread-count');
	return response.data.count;
};

export const markAsRead = async (id: number): Promise<Notification> => {
	const response = await $axios.post(`/notifications/${id}/read`);
	return response.data.notification;
};

export const markAllAsRead = async (): Promise<number> => {
	const response = await $axios.post('/notifications/read-all');
	return response.data.count;
};

export const deleteNotification = async (id: number): Promise<void> => {
	await $axios.delete(`/notifications/${id}`);
};

export const getNotificationSettings = async (): Promise<NotificationSettingsResponse> => {
	const response = await $axios.get('/notification-settings');
	return response.data;
};

export const updateNotificationSettings = async (
	payload: UpdateNotificationSettingsPayload
): Promise<NotificationSettings> => {
	const response = await $axios.put('/notification-settings', payload);
	return response.data.settings;
};

export const sendNotification = async () => {
	await $axios.post('test/web/notifications');
};
