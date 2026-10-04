import $axios from '@/plugins/axios';
import { requestCache } from '@/utils/requestCache';

export interface NotifyToken {
	id: number;
	label: string | null;
	prefix: string;
	created_at: string;
	last_used_at: string | null;
}

export interface IssuedNotifyToken extends NotifyToken {
	token: string;
}

const NOTIFY_TOKENS_KEY = 'notify-tokens';
const invalidateNotifyTokens = () => requestCache.invalidate(NOTIFY_TOKENS_KEY);

export const listNotifyTokens = async (
	useCache = true,
): Promise<NotifyToken[]> =>
	requestCache.getOrFetch(
		NOTIFY_TOKENS_KEY,
		async () => {
			const {
				data: { data },
			} = await $axios.get('/notify-tokens');
			return data;
		},
		{ ttl: 30000, cache: useCache },
	);

export const createNotifyToken = async (
	label?: string,
): Promise<IssuedNotifyToken> => {
	const {
		data: { data },
	} = await $axios.post('/notify-tokens', label ? { label } : {});
	invalidateNotifyTokens();
	return data;
};

export const revokeNotifyToken = async (id: number): Promise<void> => {
	await $axios.delete(`/notify-tokens/${id}`);
	invalidateNotifyTokens();
};

export type TestNotificationResult =
	| { kind: 'sent'; channels: string[] }
	| { kind: 'no_channels' }
	| { kind: 'deduplicated' }
	| { kind: 'rate_limited'; retryAfter: number | null }
	| { kind: 'unauthorized' }
	| { kind: 'error' };

export const buildNotifyPushUrl = (apiBaseUrl?: string): string => {
	const origin =
		typeof window !== 'undefined' ? window.location.origin : 'http://localhost';
	const base = (apiBaseUrl || '/api/').replace(/\/+$/, '');
	return new URL(`${base}/notifications/push`, origin).toString();
};

const pad = (n: number) => String(n).padStart(2, '0');

export const sendTestNotification = async (
	token: string,
	apiBaseUrl?: string,
): Promise<TestNotificationResult> => {
	const now = new Date();
	const time = `${pad(now.getHours())}:${pad(now.getMinutes())}:${pad(now.getSeconds())}`;
	try {
		const response = await fetch(buildNotifyPushUrl(apiBaseUrl), {
			method: 'POST',
			headers: {
				Authorization: `Bearer ${token}`,
				'Content-Type': 'application/json',
			},
			body: JSON.stringify({
				title: 'Test from TMGR',
				body: `Your agent notifications are set up. Sent at ${time}`,
				priority: 'normal',
				source: 'settings-test',
			}),
		});
		if (response.status === 401) return { kind: 'unauthorized' };
		if (response.status === 429) {
			const retryAfter = Number(response.headers.get('Retry-After'));
			return {
				kind: 'rate_limited',
				retryAfter: Number.isFinite(retryAfter) && retryAfter > 0 ? retryAfter : null,
			};
		}
		if (!response.ok) return { kind: 'error' };
		const { data } = await response.json();
		if (data?.status === 'deduplicated') return { kind: 'deduplicated' };
		const channels: string[] = Array.isArray(data?.channels) ? data.channels : [];
		return channels.length ? { kind: 'sent', channels } : { kind: 'no_channels' };
	} catch {
		return { kind: 'error' };
	}
};
