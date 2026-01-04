import { getWorkspaceFeatureToggles, getUserFeatureToggles, updateWorkspaceFeatureToggles, updateUserFeatureToggles } from '@/actions/tmgr/featureToggles';

export default {
    namespaced: true,
    
    state: () => ({
        workspaceToggles: {},
        userToggles: {},
        workspaceLoaded: false,
        userLoaded: false
    }),

    mutations: {
        setWorkspaceToggles(state, toggles) {
            state.workspaceToggles = toggles;
            state.workspaceLoaded = true;
        },
        setUserToggles(state, toggles) {
            state.userToggles = toggles;
            state.userLoaded = true;
        },
    },

    getters: {
        isWorkspaceFeatureEnabled: (state) => (key) => {
            if (!state.workspaceLoaded) return false;
            const toggle = state.workspaceToggles[key];
            if (!toggle) return false;
            return toggle.enabled ?? false;
        },
        isUserFeatureEnabled: (state) => (key) => {
            if (!state.userLoaded) return false;
            const toggle = state.userToggles[key];
            if (!toggle) return false;
            
            if (typeof toggle.value === 'boolean') {
                return toggle.value;
            }
            
            return true;
        },
        getUserFeatureValue: (state) => (key) => {
            const toggle = state.userToggles[key];
            return toggle?.value ?? null;
        },
        isLoaded: (state) => state.workspaceLoaded && state.userLoaded
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
        
        async enforceLandingPage({ state, dispatch }) {
            const allowed = ['list'];
            if (state.workspaceToggles?.board?.enabled) allowed.push('board');
            if (state.workspaceToggles?.dashboard?.enabled) allowed.push('dashboard');

            const current = state.userToggles?.default_landing_page?.value;
            if (!allowed.includes(current)) {
                const next = allowed[0];
                await dispatch('updateUserToggles', { default_landing_page: next });
            }
            return allowed;
        },

        async loadWorkspaceToggles({ commit, dispatch }, workspaceId) {
            try {
                const data = await getWorkspaceFeatureToggles(workspaceId);
                commit('setWorkspaceToggles', data);
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
        
        async updateWorkspaceToggles({ commit, dispatch }, { workspaceId, toggles }) {
            try {
                await updateWorkspaceFeatureToggles(workspaceId, toggles);
                const data = await getWorkspaceFeatureToggles(workspaceId);
                commit('setWorkspaceToggles', data);
                await dispatch('enforceLandingPage');
            } catch (error) {
                console.error('Failed to update workspace feature toggles:', error);
                throw error;
            }
        }
    }
};

