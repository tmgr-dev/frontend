import type { Workspace } from '@/actions/tmgr/workspaces';

// Keep in sync with the .statusbar-offset rail rule in _extensions.scss.
export const RAIL_SIDEBAR_WIDTH_CLASS = '![--sidebar-width:19.5rem]';

export const isWorkspaceRailVisible = (state: {
	isMobile: boolean;
	expanded: boolean;
	canUseApp: boolean;
}): boolean => !state.isMobile && state.expanded && state.canUseApp;

export const isActiveWorkspace = (
	workspace: Pick<Workspace, 'id'>,
	active: Pick<Workspace, 'id'> | null | undefined,
): boolean => active != null && Number(workspace.id) === Number(active.id);
