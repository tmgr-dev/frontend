import type { NewTimeEntry } from '@/actions/tmgr/timeEntries';
import {
	loadGitTrackFolders,
	loadGitTrackMode,
	type GitTrackMode,
} from '@/utils/gitTrack';
import { format } from 'date-fns';

export interface ActivitySuggestion {
	id: string;
	key: string;
	startedAt: number;
	endedAt: number;
	seconds: number;
}

export interface ActivityContext {
	userId: number | null | undefined;
	workspaceId: number | null | undefined;
	timerRunning: boolean;
	modulesKnown: boolean;
	sessionTimeOn: boolean;
}

export type EntryOutcome = 'done' | 'conflict' | 'error';

export const ACTIVITY_APPS: readonly string[] = [
	'Visual Studio Code',
	'Cursor',
	'Visual Studio',
	'Xcode',
	'IntelliJ IDEA',
	'WebStorm',
	'PyCharm',
	'GoLand',
	'RustRover',
	'PhpStorm',
	'CLion',
	'Rider',
	'DataGrip',
	'Zed',
	'Terminal',
	'iTerm2',
	'Warp',
	'Ghostty',
	'kitty',
	'WezTerm',
	'Alacritty',
	'Windows Terminal',
];

type UserId = number | string | null | undefined;

const consentKey = (userId: UserId): string | null =>
	userId == null || userId === '' ? null : `app.activity.enabled.${userId}`;

export const loadActivityConsent = (userId: UserId): boolean => {
	const key = consentKey(userId);
	if (!key) return false;
	try {
		return localStorage.getItem(key) === 'true';
	} catch {
		return false;
	}
};

export const saveActivityConsent = (userId: UserId, enabled: boolean): void => {
	const key = consentKey(userId);
	if (!key) return;
	try {
		localStorage.setItem(key, enabled ? 'true' : 'false');
	} catch {}
};

export const activityEnabledFor = ({
	consent,
	supported,
	mode,
	folders,
	modulesKnown,
	sessionTimeOn,
}: {
	consent: boolean;
	supported: boolean;
	mode: GitTrackMode;
	folders: string[];
	modulesKnown: boolean;
	sessionTimeOn: boolean;
}): boolean =>
	consent &&
	supported &&
	mode !== 'off' &&
	folders.length > 0 &&
	modulesKnown &&
	sessionTimeOn;

let provider: (() => ActivityContext) | null = null;
let supportedCache: Promise<boolean> | null = null;
let lastPushed: string | null = null;

export const registerActivityContext = (
	next: (() => ActivityContext) | null,
): void => {
	provider = next;
};

export const resetActivitySync = (): void => {
	supportedCache = null;
	lastPushed = null;
};

const invokeCommand = async <T>(
	command: string,
	args?: Record<string, unknown>,
): Promise<T> => {
	const { invoke } = await import('@tauri-apps/api/core');
	return invoke<T>(command, args);
};

export const isActivitySupported = (): Promise<boolean> => {
	supportedCache ??= invokeCommand<boolean>('activity_supported')
		.then((value) => value === true)
		.catch(() => false);
	return supportedCache;
};

const push = async (payload: {
	consent: boolean;
	collecting: boolean;
	userId: number;
	workspaceId: number;
	timerRunning: boolean;
}): Promise<void> => {
	const serialized = JSON.stringify(payload);
	if (serialized === lastPushed) return;
	lastPushed = serialized;
	try {
		await invokeCommand('activity_update', payload);
	} catch (error) {
		lastPushed = null;
		throw error;
	}
};

export const syncActivity = async (): Promise<void> => {
	if (!(await isActivitySupported())) return;
	const ctx = provider?.();
	const userId = ctx?.userId;
	const workspaceId = ctx?.workspaceId;
	const identified = !!ctx && typeof userId === 'number' && workspaceId != null;
	const consent = identified && loadActivityConsent(userId);
	const collecting =
		consent &&
		activityEnabledFor({
			consent,
			supported: true,
			mode: loadGitTrackMode(),
			folders: loadGitTrackFolders(),
			modulesKnown: ctx.modulesKnown,
			sessionTimeOn: ctx.sessionTimeOn,
		});
	await push({
		consent: identified ? consent : true,
		collecting,
		userId: identified ? userId : 0,
		workspaceId: identified ? Number(workspaceId) : 0,
		timerRunning: ctx?.timerRunning ?? true,
	});
};

export const stopActivity = async (): Promise<void> => {
	if (!(await isActivitySupported())) return;
	lastPushed = null;
	await push({
		consent: true,
		collecting: false,
		userId: 0,
		workspaceId: 0,
		timerRunning: true,
	});
	lastPushed = null;
};

export const buildTimeEntry = (
	suggestion: ActivitySuggestion,
): NewTimeEntry => {
	const span = Math.max(
		0,
		Math.floor(suggestion.endedAt - suggestion.startedAt),
	);
	return {
		client_interval_id: suggestion.id,
		source: 'activity',
		started_at: new Date(suggestion.startedAt * 1000).toISOString(),
		ended_at: new Date(suggestion.endedAt * 1000).toISOString(),
		seconds: Math.min(Math.max(0, Math.floor(suggestion.seconds)), span),
	};
};

export const entryOutcome = (status: string | undefined): EntryOutcome => {
	if (status === 'created' || status === 'duplicate') return 'done';
	if (status === 'conflict') return 'conflict';
	return 'error';
};

export const minutesOf = (seconds: number): number =>
	Math.max(1, Math.round(seconds / 60));

export const formatSuggestionRange = (
	startedAt: number,
	endedAt: number,
): { date: string; range: string } => {
	const start = new Date(startedAt * 1000);
	const end = new Date(endedAt * 1000);
	return {
		date: format(start, 'd MMM yyyy'),
		range: `${format(start, 'HH:mm')}–${format(end, 'HH:mm')}`,
	};
};

export const listActivitySuggestions = (
	userId: number,
	workspaceId: number,
): Promise<ActivitySuggestion[]> =>
	invokeCommand<ActivitySuggestion[]>('activity_suggestions', {
		userId,
		workspaceId,
	});

export const consumeActivity = (
	userId: number,
	workspaceId: number,
	suggestion: ActivitySuggestion,
): Promise<void> =>
	invokeCommand('activity_consume', {
		userId,
		workspaceId,
		key: suggestion.key,
		startedAt: suggestion.startedAt,
		endedAt: suggestion.endedAt,
	});

export const forgetActivity = (): Promise<void> =>
	invokeCommand('activity_forget');
