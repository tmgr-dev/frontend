import $axios from '@/plugins/axios';
import type { TimeDailyDay } from '@/types/dashboard';
import { requestCache } from '@/utils/requestCache';

/**
 * Workspace-wide tracked time per UTC day (GET /workspaces/{id}/time/daily, at most 31 days).
 * Behind the `exports` module: may reject with 403 feature_disabled.
 */
export const getWorkspaceTimeDaily = async (
	workspaceId: number,
	from: string,
	to: string,
	useCache = true,
): Promise<TimeDailyDay[]> => {
	const key = `workspace-${workspaceId}-time-daily-${from}-${to}`;
	if (!useCache) requestCache.invalidate(key);
	return requestCache.getOrFetch<TimeDailyDay[]>(
		key,
		async () => {
			const { data } = await $axios.get(
				`/workspaces/${workspaceId}/time/daily`,
				{
					params: { from, to, tz: 'UTC' },
				},
			);
			const days = data?.data?.days;
			return Array.isArray(days)
				? days.map((d: TimeDailyDay) => ({
						date: String(d.date),
						total_s: Number(d.total_s) || 0,
						items: Array.isArray(d.items) ? d.items : [],
					}))
				: [];
		},
		{ ttl: 60000 },
	);
};
