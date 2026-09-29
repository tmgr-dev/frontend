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
