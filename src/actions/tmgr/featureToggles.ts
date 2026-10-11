import { invalidateWorkspaceModules } from '@/actions/tmgr/modules';
import $axios from '@/plugins/axios';
import requestCache from '@/utils/requestCache';

export interface FeatureToggle {
	key: string;
	name: string;
	group: string;
	enabled?: boolean;
	value?: string | boolean;
	default?: string | boolean;
	type?: 'boolean' | 'select';
	options?: string[];
}

const USER_TOGGLES_KEY = 'user-feature-toggles';

export { getWorkspaceFeatureToggles } from '@/actions/tmgr/modules';

export const updateWorkspaceFeatureToggles = async (
	workspaceId: number,
	features: Record<string, boolean>,
): Promise<void> => {
	await $axios.put(`/workspaces/${workspaceId}/feature-toggles`, { features });
	invalidateWorkspaceModules(workspaceId);
};

export const getUserFeatureToggles = async (): Promise<
	Record<string, FeatureToggle>
> => {
	return requestCache.getOrFetch(
		USER_TOGGLES_KEY,
		async () => {
			const {
				data: { data },
			} = await $axios.get('/user/feature-toggles');
			return data;
		},
		{ cache: false },
	);
};

export const updateUserFeatureToggles = async (
	features: Record<string, boolean | string>,
): Promise<void> => {
	await $axios.put('/user/feature-toggles', { features });
	requestCache.invalidate(USER_TOGGLES_KEY);
};
