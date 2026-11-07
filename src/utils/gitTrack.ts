import { sameTask, type TrayTask } from '@/utils/desktopTray';

export type GitTrackMode = 'ask' | 'auto' | 'off';
export type GitTrackDecision = 'ignore' | 'ask' | 'switch';

export interface GitTrackTask {
	id: number;
	workspaceId?: number | null;
	statusType?: string | null;
}

export const MAX_GIT_TRACK_FOLDERS = 20;

const MODE_KEY = 'app.gitTrack.mode';
const FOLDERS_KEY = 'app.gitTrack.folders';
const MODES: GitTrackMode[] = ['ask', 'auto', 'off'];
const SKIPPED_IN_AUTO = ['completed', 'archived'];

export const loadGitTrackMode = (): GitTrackMode => {
	try {
		const stored = localStorage.getItem(MODE_KEY);
		return MODES.includes(stored as GitTrackMode)
			? (stored as GitTrackMode)
			: 'ask';
	} catch {
		return 'ask';
	}
};

export const saveGitTrackMode = (mode: GitTrackMode): void => {
	try {
		localStorage.setItem(MODE_KEY, mode);
	} catch {
		/* storage unavailable: the mode falls back to ask after a restart */
	}
};

export const loadGitTrackFolders = (): string[] => {
	try {
		const parsed = JSON.parse(localStorage.getItem(FOLDERS_KEY) || '[]');
		return Array.isArray(parsed)
			? parsed
					.filter((folder): folder is string => typeof folder === 'string')
					.slice(0, MAX_GIT_TRACK_FOLDERS)
			: [];
	} catch {
		return [];
	}
};

export const saveGitTrackFolders = (folders: string[]): void => {
	try {
		localStorage.setItem(
			FOLDERS_KEY,
			JSON.stringify(folders.slice(0, MAX_GIT_TRACK_FOLDERS)),
		);
	} catch {
		/* storage unavailable: the folder list won't survive a restart */
	}
};

const updateWatchedFolders = async (folders: string[]): Promise<void> => {
	const { invoke } = await import('@tauri-apps/api/core');
	await invoke('git_track_update', { folders });
};

export const syncGitTrack = (): Promise<void> =>
	updateWatchedFolders(
		loadGitTrackMode() === 'off' ? [] : loadGitTrackFolders(),
	);

export const unwatchGitTrack = (): Promise<void> => updateWatchedFolders([]);

export const decideGitTrack = ({
	mode,
	task,
	runningTasks,
}: {
	mode: GitTrackMode;
	task: GitTrackTask | null | undefined;
	runningTasks: Array<TrayTask | { id: number; workspace_id?: number | null }>;
}): GitTrackDecision => {
	if (mode === 'off' || !task) return 'ignore';
	if (runningTasks.some((running) => sameTask(running, task))) return 'ignore';
	if (mode === 'ask') return 'ask';
	if (task.statusType && SKIPPED_IN_AUTO.includes(task.statusType)) {
		return 'ignore';
	}
	return 'switch';
};
