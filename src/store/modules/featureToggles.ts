// @ts-nocheck
import {
	getUserFeatureToggles,
	updateUserFeatureToggles,
	updateWorkspaceFeatureToggles,
} from '@/actions/tmgr/featureToggles';
import {
	getLegacyWorkspaceFeatureToggles,
	getWorkspaceModules,
} from '@/actions/tmgr/modules';
import {
	isEntryVisible,
	modulesToMap,
	readModulesCache,
	writeModulesCache,
} from '@/utils/modules';
import { allowedLandings } from '@/utils/moduleSurfaces';

export default {
	namespaced: true,

	state: () => ({
		workspaceToggles: {},
		userToggles: {},
		workspaceLoaded: false,
		userLoaded: false,
		loadedWorkspaceId: null,
		modulesMeta: null,
	}),

	mutations: {
		setWorkspaceToggles(state, toggles) {
			state.workspaceToggles = toggles;
			state.workspaceLoaded = true;
			state.loadedWorkspaceId = null;
			state.modulesMeta = null;
		},
		setWorkspaceModules(state, { workspaceId, payload }) {
			const toggles = modulesToMap(payload);
			state.workspaceToggles = toggles;
			state.workspaceLoaded = true;
			state.loadedWorkspaceId = workspaceId;
			state.modulesMeta = {
				configured: payload.configured,
				canManage: payload.canManage,
				enforcement: payload.enforcement,
				packs: payload.packs,
				presets: payload.presets,
			};
			writeModulesCache(workspaceId, toggles);
		},
		setUserToggles(state, toggles) {
			state.userToggles = toggles;
			state.userLoaded = true;
		},
	},

	getters: {
		isFeatureEnabled: (state, getters, rootState, rootGetters) => (key) => {
			const currentId = rootGetters?.currentWorkspaceId;
			const fresh =
				state.workspaceLoaded &&
				(state.loadedWorkspaceId == null ||
					currentId == null ||
					state.loadedWorkspaceId == currentId);
			const workspaceMap = fresh
				? state.workspaceToggles
				: readModulesCache(currentId);
			const entry =
				workspaceMap?.[key] ??
				(state.userLoaded ? state.userToggles[key] : null);
			return isEntryVisible(entry);
		},
		isHiddenByMe: (state) => (key) =>
			state.workspaceToggles?.[key]?.hidden === true,
		canManageModules: (state, getters, rootState, rootGetters) => {
			if (state.modulesMeta) return state.modulesMeta.canManage;
			const workspace = rootGetters?.currentWorkspace;
			return !!workspace && workspace.user_id === rootState?.user?.id;
		},
		isWorkspaceFeatureEnabled: (state, getters) => (key) =>
			getters.isFeatureEnabled(key),
		isUserFeatureEnabled: (state, getters) => (key) =>
			getters.isFeatureEnabled(key),
		getUserFeatureValue: (state) => (key) => {
			const toggle = state.userToggles[key];
			return toggle?.value ?? null;
		},
		isLoaded: (state) => state.workspaceLoaded && state.userLoaded,
	},

	actions: {
		async loadUserToggles({ commit }) {
			try {
				const data = await getUserFeatureToggles();
				commit('setUserToggles', data);
			} catch (error) {
				console.error('Failed to load user feature toggles:', error);
			}
		},

		async enforceLandingPage({ state, dispatch, getters }) {
			const allowed = allowedLandings(getters.isFeatureEnabled);

			const current = state.userToggles?.default_landing_page?.value;
			if (!allowed.includes(current)) {
				const next = allowed[0];
				await dispatch('updateUserToggles', { default_landing_page: next });
			}
			return allowed;
		},

		async fetchWorkspaceModules({ commit }, workspaceId) {
			const payload = await getWorkspaceModules(workspaceId);
			if (payload) {
				commit('setWorkspaceModules', { workspaceId, payload });
				return payload;
			}
			commit(
				'setWorkspaceToggles',
				await getLegacyWorkspaceFeatureToggles(workspaceId),
			);
			return null;
		},

		async loadWorkspaceToggles({ dispatch }, workspaceId) {
			try {
				await dispatch('fetchWorkspaceModules', workspaceId);
				await dispatch('enforceLandingPage');
			} catch (error) {
				console.error('Failed to load workspace feature toggles:', error);
			}
		},

		async updateUserToggles({ commit }, toggles) {
			try {
				await updateUserFeatureToggles(toggles);
				const data = await getUserFeatureToggles();
				commit('setUserToggles', data);
			} catch (error) {
				console.error('Failed to update user feature toggles:', error);
				throw error;
			}
		},

		async updateWorkspaceToggles({ dispatch }, { workspaceId, toggles }) {
			try {
				await updateWorkspaceFeatureToggles(workspaceId, toggles);
				await dispatch('fetchWorkspaceModules', workspaceId);
				await dispatch('enforceLandingPage');
			} catch (error) {
				console.error('Failed to update workspace feature toggles:', error);
				throw error;
			}
		},
	},
};
