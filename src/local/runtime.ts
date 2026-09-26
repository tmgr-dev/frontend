import { migrate } from './schema';
import type { LocalContext, LocalDb, LocalUser, LocalWorkspace } from './types';

const ACTIVE_KEY = 'local.activeWorkspace';

const invoke = async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
	const core = await import('@tauri-apps/api/core');
	return core.invoke<T>(command, args);
};

let workspaces: LocalWorkspace[] = [];
let listed: Promise<LocalWorkspace[]> | null = null;
const databases = new Map<string, Promise<LocalDb>>();

const readActive = (): string | null => {
	try {
		return localStorage.getItem(ACTIVE_KEY);
	} catch {
		return null;
	}
};

let activeCode = readActive();

export const listLocalWorkspaces = (refresh = false): Promise<LocalWorkspace[]> => {
	if (!listed || refresh) {
		listed = invoke<LocalWorkspace[]>('local_workspaces_list')
			.then((list) => (workspaces = Array.isArray(list) ? list : []))
			.catch((error) => {
				listed = null;
				console.error('Failed to list local workspaces', error);
				return workspaces;
			});
	}
	return listed;
};

export const createLocalWorkspace = async (name: string): Promise<LocalWorkspace> => {
	const created = await invoke<LocalWorkspace>('local_workspace_create', { name });
	await listLocalWorkspaces(true);
	return created;
};

export const activeLocalWorkspace = (): LocalWorkspace | null =>
	activeCode ? (workspaces.find((w) => w.code === activeCode) ?? null) : null;

/** Whether a local workspace is active, even before the folder list has been read. */
export const hasActiveLocalWorkspace = (): boolean => activeCode !== null;

export const setActiveLocalWorkspace = (workspace: LocalWorkspace | null) => {
	activeCode = workspace?.code ?? null;
	try {
		if (activeCode) localStorage.setItem(ACTIVE_KEY, activeCode);
		else localStorage.removeItem(ACTIVE_KEY);
	} catch {
		/* storage unavailable: the choice lasts until restart */
	}
};

export const localWorkspaceById = async (id: number) =>
	(await listLocalWorkspaces()).find((w) => w.id === id) ?? null;

const openDatabase = async (workspace: LocalWorkspace): Promise<LocalDb> => {
	// SQL runs in Rust against this workspace's own file only; see src-tauri/src/local_db.rs.
	const db: LocalDb = {
		select: (sql, params = []) =>
			invoke('local_db_select', { code: workspace.code, sql, params }),
		execute: (sql, params = []) =>
			invoke('local_db_execute', { code: workspace.code, sql, params }),
	};
	const version = await migrate(db, new Date().toISOString(), async () => {
		await invoke('local_db_backup', { code: workspace.code });
	});
	if (version !== workspace.schema_version) {
		await invoke('local_workspace_set_schema', { code: workspace.code, version });
	}
	return db;
};

export const localContext = async (
	workspace: LocalWorkspace,
	user: LocalUser,
): Promise<LocalContext> => {
	let db = databases.get(workspace.code);
	if (!db) {
		db = openDatabase(workspace);
		databases.set(workspace.code, db);
		db.catch(() => databases.delete(workspace.code));
	}
	return { db: await db, workspace, user, now: () => new Date() };
};
