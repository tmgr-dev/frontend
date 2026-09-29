import { isDesktopApp } from '@/utils/desktop';

/**
 * Keeps the desktop app's local-workspace runtime (local/install.ts's request routing) in step
 * with this tab's resolved workspace, for every path that can change it — not just the sidebar's
 * own switch action, but the router guard and boot sequence reacting to a typed URL, Back/Forward,
 * or a cross-workspace link. A no-op outside the desktop app.
 */
export const syncActiveLocalWorkspace = async (
	workspaceId: number | null,
): Promise<void> => {
	if (!isDesktopApp() || workspaceId == null) return;
	const { hasActiveLocalWorkspace, localWorkspaceById, setActiveLocalWorkspace } =
		await import('@/local/runtime');
	if (workspaceId < 0) {
		const workspace = await localWorkspaceById(workspaceId);
		setActiveLocalWorkspace(workspace);
	} else if (hasActiveLocalWorkspace()) {
		setActiveLocalWorkspace(null);
	}
};
