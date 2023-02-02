import $axios from 'src/plugins/axios';
import store from 'src/store';
import axios from 'axios';

export interface User {}

export const getUser = async () => {
	const {
		data: { data },
	} = await $axios.get('user', {
		headers: {
			Authorization: `Bearer ${store.getters.token.token}`,
		},
	});
	store.commit('user', data);
};

export interface UserSettings {}

export const getUserSettings = async () => {
	const {
		data: { data },
	} = await axios.get(`user/settings`);

	if (data?.settings) {
		store.commit('setUserSettings', data.settings);
	}
};

export interface WorkspaceStatuses {}

export const getWorkspaceStatuses = async () => {
	const {
		data: { data },
	} = await axios.get(`workspaces/statuses`);

	store.commit('setStatuses', data);
};
