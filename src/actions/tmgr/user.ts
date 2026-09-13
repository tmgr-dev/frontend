import { FormSetting, Setting } from '@/actions/tmgr/settings';
import $axios from '@/plugins/axios';
import store from '@/store';
import { pickThemeFromSettings } from '@/theme/reconcile';
import requestCache from '@/utils/requestCache';

export { pickThemeFromSettings };

export interface User {
	id: number;
	email: string;
	role: number;
	name: string;
	settings: FormSetting[];
}

export const getUser = async () => {
	// Coalesced but never cached: callers after a save expect fresh data, while the bootstrap fires
	// this from several places at once and used to send the same request two or three times.
	const data = await requestCache.getOrFetch<User>(
		'user',
		async () => {
			const {
				data: { data: user },
			} = await $axios.get('user');

			return user;
		},
		{ cache: false },
	);

	store.commit('setUser', data);

	const picked = pickThemeFromSettings(data?.settings);
	if (picked.theme !== undefined) store.commit('setTheme', picked.theme);
	if (picked.colorScheme !== undefined)
		store.commit('setColorScheme', picked.colorScheme);

	return data;
};

export const updateUser = async (payload: User) => {
	const {
		data: { data },
	} = await $axios.put('user', payload);

	return data;
};

export interface UserSettings {}

export const getUserSettings = async () => {
	const {
		data: { data },
	} = await $axios.get('user/settings');

	if (data?.settings) {
		store.commit('setUserSettings', data.settings);
	}
};

export const updateUserSettings = async <T>(payload: T) => {
	const {
		data: { data },
	} = await $axios.put('user/settings', payload);

	return data;
};

export const patchUserSettings = async (settings: Record<string, any>) => {
	const {
		data: { data },
	} = await $axios.patch('user/settings', { settings });

	return data;
};

export const getUserSettingsV2 = async (): Promise<Setting[]> => {
	const {
		data: { data },
	} = await $axios.get('v2/user/settings');

	return data;
};

export const updateUserSettingsV2 = async (payload: UserSettings) => {
	const {
		data: { data },
	} = await $axios.put('v2/user/settings', payload);

	return data;
};

export const persistThemeSettings = async (
	theme: string,
	colorScheme: string,
) => {
	const current = Array.isArray(store.state.user?.settings)
		? store.state.user.settings
		: [];
	const payload = current.map((s: any) => {
		if (s.key === 'theme') return { id: s.id, value: theme };
		if (s.key === 'colorScheme') return { id: s.id, value: colorScheme };
		return { id: s.id, value: s.value };
	});
	const updatedUser = await updateUserSettingsV2(payload);
	store.commit('setUser', updatedUser);
};
