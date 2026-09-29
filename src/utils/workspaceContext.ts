export const WORKSPACE_SESSION_KEY = 'tmgr:tabWorkspaceId';
export const WORKSPACE_LOCAL_KEY = 'tmgr:lastWorkspaceId';

export interface StorageLike {
	getItem(key: string): string | null;
	setItem(key: string, value: string): void;
	removeItem(key: string): void;
}

export interface WorkspaceLike {
	id: number | string;
}

export interface SettingEntry {
	id?: number | string;
	key?: string;
	value?: unknown;
	[extra: string]: unknown;
}

const toId = (raw: string | null): number | null => {
	if (raw == null || raw === '') return null;
	const value = Number(raw);
	return Number.isFinite(value) ? value : null;
};

export const readWorkspaceId = (
	storage: StorageLike | undefined | null,
	key: string,
): number | null => {
	if (!storage) return null;
	try {
		return toId(storage.getItem(key));
	} catch {
		return null;
	}
};

export const writeWorkspaceId = (
	storage: StorageLike | undefined | null,
	key: string,
	id: number | null,
): void => {
	if (!storage) return;
	try {
		if (id == null) storage.removeItem(key);
		else storage.setItem(key, String(id));
	} catch {
		/* storage unavailable (private mode, quota): the choice lasts for this render only */
	}
};

export const isKnownWorkspaceId = (
	id: number | null | undefined,
	workspaces: WorkspaceLike[] | undefined,
): boolean =>
	id != null &&
	Array.isArray(workspaces) &&
	workspaces.some((w) => Number(w.id) === Number(id));

/**
 * The tab's workspace, in priority order: the URL, this tab's own prior choice, the browser's last
 * choice, then the account default. A candidate the workspace list doesn't recognise is skipped —
 * unless the list hasn't loaded yet, when it can't be checked and is trusted provisionally.
 */
export const resolveWorkspaceId = ({
	urlWorkspaceId = null,
	sessionWorkspaceId = null,
	lastWorkspaceId = null,
	defaultWorkspaceId = null,
	workspaces = [],
}: {
	urlWorkspaceId?: number | null;
	sessionWorkspaceId?: number | null;
	lastWorkspaceId?: number | null;
	defaultWorkspaceId?: number | null;
	workspaces?: WorkspaceLike[];
}): number | null => {
	const candidates = [
		urlWorkspaceId,
		sessionWorkspaceId,
		lastWorkspaceId,
		defaultWorkspaceId,
	];
	for (const candidate of candidates) {
		if (candidate == null) continue;
		if (!workspaces.length || isKnownWorkspaceId(candidate, workspaces)) {
			return Number(candidate);
		}
	}
	return null;
};

/** Only a positive, currently-known cloud workspace id may travel as a header; local ids never leave the device. */
export const shouldAttachWorkspaceHeader = (
	workspaceId: number | null | undefined,
	workspaces: WorkspaceLike[] | undefined,
): boolean =>
	workspaceId != null &&
	workspaceId > 0 &&
	Array.isArray(workspaces) &&
	workspaces.length > 0 &&
	isKnownWorkspaceId(workspaceId, workspaces);

/**
 * The user's settings with current_workspace pinned to `workspaceId`, so every reader that still
 * finds it by key (instead of a dedicated getter) sees the tab's choice.
 */
export const overlayCurrentWorkspace = (
	settings: SettingEntry[] | undefined,
	workspaceId: number | null,
): SettingEntry[] | undefined => {
	if (!Array.isArray(settings) || workspaceId == null) return settings;
	let found = false;
	const next = settings.map((setting) => {
		if (setting?.key !== 'current_workspace') return setting;
		found = true;
		return { ...setting, value: workspaceId };
	});
	if (!found) next.push({ key: 'current_workspace', value: workspaceId });
	return next;
};

/**
 * A settings PUT payload with current_workspace rewritten back to the account default, unless the
 * caller explicitly means to change that default (`allowDefaultChange`). A negative (local
 * workspace) target is left alone either way: only the desktop local-switch path ever sends one,
 * and the axios adapter that receives it already keeps it off the real network.
 */
export const rewriteCurrentWorkspaceEntry = (
	payload: unknown,
	currentWorkspaceSettingId: number | string | null | undefined,
	defaultWorkspaceId: number | null,
	allowDefaultChange = false,
): unknown => {
	if (allowDefaultChange) return payload;
	if (!Array.isArray(payload) || currentWorkspaceSettingId == null) return payload;
	return payload.map((entry: SettingEntry) => {
		if (entry?.id !== currentWorkspaceSettingId) return entry;
		if (Number(entry.value) < 0) return entry;
		return { ...entry, value: defaultWorkspaceId };
	});
};
