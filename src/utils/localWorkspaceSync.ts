import { isDesktopApp } from '@/utils/desktop';

export const syncActiveLocalWorkspace = async (
	workspaceId: number | null,
): Promise<void> => {
	if (!isDesktopApp() || workspaceId == null) return;
	const {
		hasActiveLocalWorkspace,
		localWorkspaceById,
		setActiveLocalWorkspace,
	} = await import('@/local/runtime');
	if (workspaceId < 0) {
		const workspace = await localWorkspaceById(workspaceId);
		setActiveLocalWorkspace(workspace);
	} else if (hasActiveLocalWorkspace()) {
		setActiveLocalWorkspace(null);
	}
};
