export interface TaskFormReferenceDeps {
	getStatuses: () => Promise<any[]>;
	getStatusesOfWorkspace: (workspaceId: number) => Promise<any[]>;
	getCategories: () => Promise<any[]>;
	getCategoriesOfWorkspace: (workspaceId: number) => Promise<any[]>;
	getWorkspaceMembers: (workspaceId: number) => Promise<any[]>;
	getAssignablePersonas: (workspaceId: number) => Promise<any[]>;
}

export const loadTaskFormReferenceData = async (
	{
		taskWorkspaceId,
		currentWorkspaceId,
	}: {
		taskWorkspaceId: number | null;
		currentWorkspaceId: number | null;
	},
	deps: TaskFormReferenceDeps,
) => {
	const workspaceId = taskWorkspaceId ?? currentWorkspaceId;
	const [statuses, categories, members, personas] = await Promise.all([
		taskWorkspaceId
			? deps.getStatusesOfWorkspace(taskWorkspaceId)
			: deps.getStatuses(),
		taskWorkspaceId
			? deps.getCategoriesOfWorkspace(taskWorkspaceId)
			: deps.getCategories(),
		workspaceId ? deps.getWorkspaceMembers(workspaceId) : [],
		workspaceId ? deps.getAssignablePersonas(workspaceId).catch(() => []) : [],
	]);
	return { statuses, categories, members, personas };
};
