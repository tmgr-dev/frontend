import $axios from 'src/plugins/axios';
import store from 'src/store';
import { AxiosRequestConfig } from 'axios';

export interface User {}

export const getUser = async (saveToStore = false) => {
	const {
		data: { data },
	} = await $axios.get('user');

	if (saveToStore) {
		store.commit('user', data);
	}

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

export const getUserSettingsV2 = async () => {
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

export interface WorkspaceStatuses {}

export const getWorkspaceStatuses = async () => {
	const {
		data: { data },
	} = await $axios.get('workspaces/statuses');

	store.commit('setStatuses', data);
};
