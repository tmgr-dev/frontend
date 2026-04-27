import $axios from '@/plugins/axios';
import store from '@/store';
import { requestCache } from '@/utils/requestCache';
import { requestDeduplicator } from '@/utils/requestDeduplicator';
import { patchUserSettings } from '@/actions/tmgr/user';

export type PomodoroPhase = 'focus' | 'short' | 'long';

export interface PomodoroState {
	id: number;
	task_id: number;
	user_id: number;
	phase: PomodoroPhase;
	running: boolean;
	phase_started_at: string | null;
	remaining_ms: number;
	completed_pomodoros: number;
	distractions_count: number;
	updated_at: string;
}

export interface PomodoroSettings {
	focusMin: number;
	shortMin: number;
	longMin: number;
	longEvery: number;
	autoStart: boolean;
	pauseTrackingOnBreak: boolean;
	notify: 'silent' | 'system' | 'both';
	sound: string;
	volume: number;
}

export const DEFAULT_POMODORO_SETTINGS: PomodoroSettings = {
	focusMin: 25,
	shortMin: 5,
	longMin: 15,
	longEvery: 4,
	autoStart: false,
	pauseTrackingOnBreak: false,
	notify: 'system',
	sound: 'bell',
	volume: 60,
};

const stateKey = (taskId: number) => `task-pomodoro-${taskId}`;

export const getPomodoroState = async (
	taskId: number,
): Promise<PomodoroState | null> => {
	const key = stateKey(taskId);
	const cached = requestCache.get<PomodoroState | null>(key);
	if (cached !== undefined) return cached;

	return requestDeduplicator.deduplicate(key, async () => {
		const {
			data: { data },
		} = await $axios.get(`tasks/${taskId}/pomodoro`);
		requestCache.set(key, data ?? null, 30_000);
		return data ?? null;
	});
};

export const enablePomodoro = async (
	taskId: number,
): Promise<PomodoroState> => {
	const {
		data: { data },
	} = await $axios.post(`tasks/${taskId}/pomodoro`);
	requestCache.set(stateKey(taskId), data, 30_000);
	return data;
};

export const disablePomodoro = async (taskId: number): Promise<void> => {
	await $axios.delete(`tasks/${taskId}/pomodoro`);
	requestCache.invalidate(stateKey(taskId));
};

export const updatePomodoroState = async (
	taskId: number,
	patch: Partial<
		Pick<
			PomodoroState,
			| 'phase'
			| 'running'
			| 'phase_started_at'
			| 'remaining_ms'
			| 'completed_pomodoros'
			| 'distractions_count'
		>
	>,
): Promise<PomodoroState> => {
	const {
		data: { data },
	} = await $axios.put(`tasks/${taskId}/pomodoro`, patch);
	requestCache.set(stateKey(taskId), data, 30_000);
	return data;
};

export const distractPomodoro = async (
	taskId: number,
): Promise<PomodoroState> => {
	const {
		data: { data },
	} = await $axios.post(`tasks/${taskId}/pomodoro/distract`);
	requestCache.set(stateKey(taskId), data, 30_000);
	return data;
};

export const getPomodoroSettings = (): PomodoroSettings => {
	const raw = (store.state as any)?.userSettings?.pomodoro;
	return { ...DEFAULT_POMODORO_SETTINGS, ...(raw ?? {}) };
};

export const savePomodoroSettings = async (
	settings: PomodoroSettings,
): Promise<PomodoroSettings> => {
	await patchUserSettings({ pomodoro: settings });
	const current = (store.state as any)?.userSettings ?? {};
	store.commit('setUserSettings', { ...current, pomodoro: settings });
	return settings;
};
