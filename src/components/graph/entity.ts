import store from '@/store';

export const workspaceIdOf = (): number | string | null =>
	store.getters.currentWorkspace?.id ?? null;
