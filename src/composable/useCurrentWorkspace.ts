import { computed } from 'vue';
import { useStore } from 'vuex';
import { useRoute } from 'vue-router';

export interface Workspace {
	id: number;
	name: string;
	code: string;
}

export function useCurrentWorkspace() {
	const store = useStore();
	const route = useRoute();

	const workspaces = computed<Workspace[]>(() => {
		return store.state.workspaces || [];
	});

	const currentWorkspaceCode = computed<string | null>(() => {
		return (route.params.workspace_code as string) || null;
	});

	const currentWorkspaceFromRoute = computed<Workspace | null>(() => {
		if (!currentWorkspaceCode.value) return null;
		return workspaces.value.find(
			(w: Workspace) => w.code === currentWorkspaceCode.value
		) || null;
	});

	const currentWorkspaceFromSettings = computed<Workspace | null>(() => {
		const settings = store.state.user?.settings || [];
		const workspaceSetting = settings.find(
			(s: { key: string; value: any }) => s.key === 'current_workspace'
		);
		
		if (!workspaceSetting?.value) return null;
		
		return workspaces.value.find(
			(w: Workspace) => w.id === workspaceSetting.value
		) || null;
	});

	const currentWorkspace = computed<Workspace | null>(() => {
		return currentWorkspaceFromRoute.value || currentWorkspaceFromSettings.value || workspaces.value[0] || null;
	});

	const currentWorkspaceId = computed<number | null>(() => {
		return currentWorkspace.value?.id || null;
	});

	const isWorkspaceSelected = computed<boolean>(() => {
		return currentWorkspaceId.value !== null;
	});

	return {
		workspaces,
		currentWorkspace,
		currentWorkspaceId,
		currentWorkspaceCode,
		currentWorkspaceFromRoute,
		currentWorkspaceFromSettings,
		isWorkspaceSelected,
	};
}
