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

/** URL > this tab > last used > account default; unknown ids are skipped once the list is loaded. */
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

export const shouldAttachWorkspaceHeader = (
	workspaceId: number | null | undefined,
	workspaces: WorkspaceLike[] | undefined,
): boolean =>
	workspaceId != null &&
	workspaceId > 0 &&
	Array.isArray(workspaces) &&
	workspaces.length > 0 &&
	isKnownWorkspaceId(workspaceId, workspaces);

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

/** Drops a cloud current_workspace entry from a settings PUT so an ordinary save never moves the account default. */
export const withoutCurrentWorkspaceEntry = (
	payload: unknown,
	currentWorkspaceSettingId: number | string | null | undefined,
	allowDefaultChange = false,
): unknown => {
	if (allowDefaultChange) return payload;
	if (!Array.isArray(payload) || currentWorkspaceSettingId == null) return payload;
	return payload.filter(
		(entry: SettingEntry) =>
			entry?.id !== currentWorkspaceSettingId || Number(entry.value) < 0,
	);
};
