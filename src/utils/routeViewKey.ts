// Only reuse screens that explicitly synchronize their state with query changes.
// Other routes retain their existing query-sensitive initialization contract.
const queryAwareViews = new Set([
	'Board',
	'WorkspaceCategory',
	'WorkspaceCategories',
	'WorkspaceCategoryChildren',
	'WorkspaceCategoryListWithStatus',
	'FallbackCategoriesList',
	'FallbackBoard',
	'WorkspaceBoard',
	'CurrentTasksList',
	'FallbackTasksList',
	'ArchiveTasksList',
	'ArchiveTasksListWithWorkspace',
	'WorkspaceTasksList',
]);

export function routeViewKey(route: {
	name?: string | symbol | null;
	path: string;
	fullPath: string;
}): string {
	return queryAwareViews.has(String(route.name)) ? route.path : route.fullPath;
}
