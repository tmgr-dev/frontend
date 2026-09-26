export interface TrayTask {
	id: number;
	title: string;
	commonTime: number;
	startTime: number;
}

export interface TrayState {
	running: TrayTask[];
	recent: TrayTask[];
}

export interface AwayEvent {
	awaySince: number;
	awaySeconds: number;
}

interface ApiTask {
	id: number;
	title: string;
	common_time?: number | null;
	start_time?: number | null;
}

const recentKey = (userId: number) => `desktop.recentTasks.${userId}`;

export const toTrayTask = (task: ApiTask): TrayTask => ({
	id: task.id,
	title: task.title,
	commonTime: task.common_time || 0,
	startTime: task.start_time || 0,
});

export const rememberRecent = (
	previous: TrayTask[],
	running: ApiTask[],
	max = 5,
): TrayTask[] => {
	const fresh = running.map(toTrayTask);
	const rest = previous.filter((p) => !fresh.some((f) => f.id === p.id));
	return [...fresh, ...rest].slice(0, max);
};

export const buildTrayState = (
	running: ApiTask[],
	recent: TrayTask[],
): TrayState => ({
	running: running.map(toTrayTask),
	recent,
});

export const formatAway = (seconds: number): string => {
	const minutes = Math.max(1, Math.round(seconds / 60));
	const h = Math.floor(minutes / 60);
	const m = minutes % 60;
	return h > 0 ? `${h} h ${m} min` : `${m} min`;
};

export const loadRecent = (userId: number): TrayTask[] => {
	try {
		const parsed = JSON.parse(localStorage.getItem(recentKey(userId)) || '[]');
		return Array.isArray(parsed) ? parsed : [];
	} catch {
		return [];
	}
};

export const saveRecent = (userId: number, recent: TrayTask[]): void => {
	try {
		localStorage.setItem(recentKey(userId), JSON.stringify(recent));
	} catch {
		/* storage unavailable: the list just won't survive a restart */
	}
};

export const pushTrayState = async (state: TrayState): Promise<void> => {
	try {
		const { invoke } = await import('@tauri-apps/api/core');
		await invoke('tray_update', { state });
	} catch (error) {
		console.error('tray_update failed', error);
	}
};
