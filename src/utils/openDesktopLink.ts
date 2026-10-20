interface WorkspaceRef {
	id: number | string;
	code: string;
}

interface ResolvedRoute {
	name?: string | symbol | null;
	params?: Record<string, unknown>;
	matched: { path: string; name?: unknown }[];
}

export const OPEN_ROUTE_NAMES: ReadonlySet<string> = new Set([
	'WorkspaceDashboard',
	'WorkspaceTasksList',
	'WorkspaceBoard',
	'WorkspaceMap',
	'Notifications',
	'ArchiveTasksListWithWorkspace',
	'WorkspaceFiles',
	'WorkspaceMember',
	'WorkspacePages',
	'WorkspacePage',
	'WorkspaceCategories',
	'WorkspaceCategoryListWithStatus',
	'WorkspaceCategoryChildren',
	'WorkspaceCategory',
	'WorkspaceCategoryTask',
	'WorkspaceTask',
]);

const NUMERIC_PARAM: Record<string, string> = {
	WorkspaceTask: 'task_id',
	WorkspaceCategoryTask: 'task_number',
};

const OPEN_PATH = /^\/[A-Za-z0-9_-]{1,64}(\/[A-Za-z0-9_-]{1,120}){1,3}$/;

export const parseOpenPath = (
	path: string,
	resolve: (p: string) => ResolvedRoute,
): string | null => {
	if (typeof path !== 'string' || !OPEN_PATH.test(path)) return null;
	let resolved: ResolvedRoute;
	try {
		resolved = resolve(path);
	} catch {
		return null;
	}
	if (typeof resolved.name !== 'string' || !OPEN_ROUTE_NAMES.has(resolved.name))
		return null;
	const numericParam = NUMERIC_PARAM[resolved.name];
	if (
		numericParam &&
		!/^\d{1,15}$/.test(String(resolved.params?.[numericParam]))
	)
		return null;
	return path;
};

export const buildOpenUrl = (path: string | null): string =>
	path ? `tmgr://open?path=${encodeURIComponent(path)}` : 'tmgr://open';

export const DESKTOP_BAR_PLATFORMS = ['macos'];
export const DESKTOP_BAR_DISMISS_KEY = 'tmgr:desktop-bar-dismissed';

const DAY_MS = 24 * 60 * 60 * 1000;
const SEEN_WINDOW_MS = 30 * DAY_MS;

export interface DesktopBarNav {
	platform?: string;
	userAgent?: string;
	maxTouchPoints?: number;
	userAgentData?: { platform?: string };
}

export interface DesktopBarInput {
	desktopSeenAt: string | null | undefined;
	now: number;
	isDesktopApp: boolean;
	loggedIn: boolean;
	userLoaded: boolean;
	dismissedForUserId: string | null;
	userId: number | string | null;
	nav: DesktopBarNav;
}

const browserOs = (nav: DesktopBarNav): string | null => {
	const ua = nav.userAgent ?? '';
	if (/iPhone|iPad|iPod|Android|Mobile/i.test(ua)) return null;
	if ((nav.maxTouchPoints ?? 0) > 1) return null;
	const hint = nav.userAgentData?.platform;
	const raw = hint || nav.platform || '';
	if (/^mac/i.test(raw)) return 'macos';
	return null;
};

export const desktopBarVisible = (input: DesktopBarInput): boolean => {
	if (input.isDesktopApp || !input.loggedIn || !input.userLoaded) return false;
	if (input.userId == null) return false;
	if (input.dismissedForUserId === String(input.userId)) return false;
	if (!input.desktopSeenAt) return false;
	const seen = Date.parse(input.desktopSeenAt);
	if (Number.isNaN(seen)) return false;
	const age = input.now - seen;
	if (age > SEEN_WINDOW_MS || age < -DAY_MS) return false;
	const os = browserOs(input.nav);
	return os !== null && DESKTOP_BAR_PLATFORMS.includes(os);
};

export const OPEN_LINK_MESSAGES = {
	invalid: "Can't open this link",
	needsCloud:
		'That page is in a cloud workspace. Sign in to a cloud account to open it.',
	unavailable: 'That page is not available in this account.',
};

export interface OpenAppPathDeps {
	resolve: (p: string) => ResolvedRoute;
	loadWorkspaces: () => Promise<WorkspaceRef[]>;
	isLocalOnly: () => boolean;
	showMain: () => Promise<unknown>;
	navigate: (path: string) => Promise<unknown>;
	notify: (message: string) => void;
}

export const openAppPath = async (
	link: { path: string | null },
	deps: OpenAppPathDeps,
): Promise<boolean> => {
	await deps.showMain();
	if (link.path === null) return true;
	const path = parseOpenPath(link.path, deps.resolve);
	if (!path) {
		deps.notify(OPEN_LINK_MESSAGES.invalid);
		return false;
	}
	const code = path.split('/')[1];
	const workspaces = await deps.loadWorkspaces().catch(() => []);
	if (!workspaces.some((w) => w.code === code)) {
		deps.notify(
			deps.isLocalOnly()
				? OPEN_LINK_MESSAGES.needsCloud
				: OPEN_LINK_MESSAGES.unavailable,
		);
		return false;
	}
	await deps.navigate(path);
	return true;
};
