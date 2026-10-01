interface WorkspaceRef {
	id: number | string;
	code: string;
}

export interface OpenPageLinkDeps {
	loadWorkspaces: () => Promise<WorkspaceRef[]>;
	currentWorkspaceId: () => number | string | null | undefined;
	resolvePageId: (slug: string) => Promise<number | null>;
	focusPageWindow: (pageId: number, workspaceCode: string) => Promise<boolean>;
	showMain: () => Promise<unknown>;
	navigate: (path: string) => Promise<unknown>;
}

export const openPageLink = async (
	link: { workspaceCode: string; slug: string },
	deps: OpenPageLinkDeps,
): Promise<boolean> => {
	const workspace = (await deps.loadWorkspaces()).find(
		(w) => w.code === link.workspaceCode,
	);
	if (!workspace) return false;
	if (Number(workspace.id) === Number(deps.currentWorkspaceId())) {
		const pageId = await deps.resolvePageId(link.slug).catch(() => null);
		if (
			pageId != null &&
			(await deps.focusPageWindow(pageId, workspace.code).catch(() => false))
		) {
			return true;
		}
	}
	await deps.showMain();
	await deps.navigate(`/${workspace.code}/pages/${link.slug}`);
	return true;
};
