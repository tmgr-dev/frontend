import $axios from '@/plugins/axios';
import store from '@/store';
import { FormSetting } from '@/actions/tmgr/settings';

export interface User {
	id: number;
	email: string;
	role: number;
	name: string;
	settings: FormSetting[];
}

export const getUser = async () => {
	const {
		data: { data },
	} = await $axios.get('user');

	store.commit('setUser', data);

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
