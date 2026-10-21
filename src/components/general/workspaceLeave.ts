type LeavableWorkspace = {
	user_id?: number | string | null;
	type?: string;
	is_default?: boolean;
	is_local?: boolean;
};

export const canLeaveWorkspace = (
	workspace: LeavableWorkspace,
	currentUserId: number | string | null | undefined,
): boolean => {
	if (workspace.is_local) return false;
	if (workspace.type === 'default' || workspace.is_default) return false;
	if (workspace.user_id == null || currentUserId == null) return false;
	return Number(workspace.user_id) !== Number(currentUserId);
};
