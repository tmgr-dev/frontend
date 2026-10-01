import { desktopWindowLabel } from '@/utils/desktop';
import { isDetachedWindowLabel } from '@/utils/taskWindow';
import { migrate } from './schema';
import type { LocalActor, LocalContext, LocalDb, LocalFiles, LocalUser, LocalWorkspace } from './types';

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
	if (isDetachedWindowLabel(desktopWindowLabel())) return;
	try {
		if (activeCode) localStorage.setItem(ACTIVE_KEY, activeCode);
		else localStorage.removeItem(ACTIVE_KEY);
	} catch {
		/* storage unavailable: the choice lasts until restart */
	}
};

/** This window only, never persisted: follow the local workspace the app has open, or none so calls go to the cloud. */
export const followActiveLocalWorkspace = (follow = true) => {
	activeCode = follow ? readActive() : null;
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

const filesOf = (workspace: LocalWorkspace): LocalFiles => {
	const url = (key: string) =>
		(window as any).__TAURI_INTERNALS__.convertFileSrc(`${workspace.code}/${key}`, 'tmgrfile');
	return {
		url,
		read: async (key) => {
			const response = await fetch(url(key));
			if (!response.ok) throw new Error(`file ${key}: ${response.status}`);
			return response.blob();
		},
		remove: async (key) => {
			await fetch(url(key), { method: 'DELETE' });
		},
	};
};

export const localContext = async (
	workspace: LocalWorkspace,
	user: LocalUser,
	actor?: LocalActor,
): Promise<LocalContext> => {
	let db = databases.get(workspace.code);
	if (!db) {
		db = openDatabase(workspace);
		databases.set(workspace.code, db);
		db.catch(() => databases.delete(workspace.code));
	}
	return { db: await db, workspace, user, now: () => new Date(), files: filesOf(workspace), actor };
};

/** Stores an attachment of the active local workspace over IPC (WKWebView drops fetch bodies to custom schemes). */
export const writeLocalFile = async (key: string, file: Blob): Promise<void> => {
	const workspace = activeLocalWorkspace();
	if (!workspace) throw new Error('No local workspace is open');
	const core = await import('@tauri-apps/api/core');
	await core.invoke('local_file_write', new Uint8Array(await file.arrayBuffer()), {
		headers: { 'x-tmgr-target': `${workspace.code}/${key}` },
	});
};

const stamp = (date: Date) =>
	date.toISOString().slice(0, 19).replace('T', '-').replace(/:/g, '');

/** Writes the whole local workspace as Markdown into its exports/ folder and shows it in Finder. */
export const exportLocalWorkspace = async (author: string): Promise<string> => {
	const workspace = activeLocalWorkspace();
	if (!workspace) throw new Error('No local workspace is open');
	const { workspaceExport } = await import('./export');
	const ctx = await localContext(workspace, { id: 0, name: author, email: '' });
	const now = new Date();
	const files = await workspaceExport(ctx.db, workspace.name, author, now.toLocaleString());
	const folder = stamp(now);
	const dir = await invoke<string>('local_export_write', { code: workspace.code, folder, files });
	await invoke('local_reveal', { code: workspace.code, relative: `exports/${folder}/README.md` });
	return dir;
};

/** Writes one task to exports/tasks/ of the local workspace and shows the file in Finder. */
export const exportLocalTask = async (taskId: number, author: string): Promise<void> => {
	const workspace = activeLocalWorkspace();
	if (!workspace) throw new Error('No local workspace is open');
	const { taskExport } = await import('./export');
	const ctx = await localContext(workspace, { id: 0, name: author, email: '' });
	const file = await taskExport(ctx.db, taskId, author);
	if (!file) throw new Error('Task not found');
	await invoke('local_export_write', { code: workspace.code, folder: 'tasks', files: [file] });
	await invoke('local_reveal', { code: workspace.code, relative: `exports/tasks/${file.path}` });
};

export const revealLocalWorkspace = async (): Promise<void> => {
	const workspace = activeLocalWorkspace();
	if (workspace) await invoke('local_reveal', { code: workspace.code, relative: null });
};
