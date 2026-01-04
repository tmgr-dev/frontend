import { computed } from 'vue';
import { useStore } from 'vuex';

const WORKSPACE_FEATURE_KEYS = [
    'categories', 'board', 'dashboard', 'daily_routines',
    'task.comments', 'task.countdown', 'task.files', 'task.relations',
    'task.checkpoints', 'task.assignees', 'notifications.push',
    'notifications.email', 'exports'
];

const USER_FEATURE_KEYS = [
    'default_landing_page', 'workspaces', 'board.category_filter',
    'board.user_filter', 'board.search_input', 'ui.tooltips', 'notifications'
];

export function useFeatureToggles() {
    const store = useStore();

    const isWorkspaceFeatureEnabled = (key: string): boolean => {
        return store.getters['featureToggles/isWorkspaceFeatureEnabled'](key);
    };

    const isUserFeatureEnabled = (key: string): boolean => {
        return store.getters['featureToggles/isUserFeatureEnabled'](key);
    };

    const getUserFeatureValue = (key: string): string | boolean => {
        return store.getters['featureToggles/getUserFeatureValue'](key);
    };

    const isFeatureEnabled = (key: string): boolean => {
        if (WORKSPACE_FEATURE_KEYS.includes(key)) {
            return isWorkspaceFeatureEnabled(key);
        }
        if (USER_FEATURE_KEYS.includes(key)) {
            return isUserFeatureEnabled(key);
        }
        return true;
    };

    const isLoaded = computed(() => store.getters['featureToggles/isLoaded']);

    return {
        isWorkspaceFeatureEnabled,
        isUserFeatureEnabled,
        getUserFeatureValue,
        isFeatureEnabled,
        isLoaded
    };
}

