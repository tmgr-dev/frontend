import type { AllowedEntry } from '@/utils/agentApprovalPolicy';
import { desktopPlatform, isDesktopApp } from '@/utils/desktop';
import { Channel, invoke } from '@tauri-apps/api/core';

export type RunnerStateKind =
	| 'starting'
	| 'working'
	| 'waiting'
	| 'blocked'
	| 'exited';

export type RunnerExitDetail = 'done' | 'failed' | 'interrupted';

export interface RunnerState {
	kind: RunnerStateKind;
	sinceMs: number;
	detail: RunnerExitDetail | null;
	exitCode?: number | null;
}

export interface RunnerSession {
	id: string;
	name: string;
	profile: string | null;
	task?: { workspaceId: number; taskId: number } | null;
	state: RunnerState;
	createdMs: number;
}

export type RunnerStatusState =
	| 'disabled'
	| 'absent'
	| 'incompatible'
	| 'ready';

export interface RunnerStatus {
	state: RunnerStatusState;
	version?: string;
	caps: string[];
	message?: string;
}

export interface RunnerProfile {
	id: string;
	label: string;
}

export type WatchEvent =
	| { type: 'state'; session: string; state: RunnerState }
	| { type: 'sessions'; sessions: RunnerSession[] }
	| { type: 'down'; message: string };

export interface StartRunArgs {
	workspaceId: number;
	taskId: number;
	profile: string;
}

export interface TermState {
	kind: RunnerStateKind;
	sinceMs: number;
	detail: string | null;
}

export type TermFrame =
	| { type: 'data'; seq: number; data: string }
	| { type: 'state'; state: TermState }
	| { type: 'scroll'; offset: number; max: number }
	| { type: 'closed'; reason: string };

export interface TermAttachResult {
	session: string;
	name: string;
	task: { workspaceId: number; taskId: number } | null;
	state: TermState;
}

export const isRunnerSupported = (): boolean =>
	isDesktopApp() && desktopPlatform() !== 'windows';

export const runnerEnabled = (): Promise<boolean> =>
	invoke<boolean>('runner_enabled');

export const runnerSetEnabled = (enabled: boolean): Promise<void> =>
	invoke<void>('runner_set_enabled', { enabled });

export const runnerStatus = (): Promise<RunnerStatus> =>
	invoke<RunnerStatus>('runner_status');

export const runnerProfiles = (): Promise<RunnerProfile[]> =>
	invoke<RunnerProfile[]>('runner_profiles');

export const runnerProjectFolder = (
	workspaceId: number,
): Promise<string | null> =>
	invoke<string | null>('runner_project_folder', { workspaceId });

export const runnerPickFolder = (workspaceId: number): Promise<string | null> =>
	invoke<string | null>('runner_pick_folder', { workspaceId });

export const runnerSessions = (): Promise<RunnerSession[]> =>
	invoke<RunnerSession[]>('runner_sessions');

export const runnerStartRun = (args: StartRunArgs): Promise<RunnerSession> =>
	invoke<RunnerSession>('runner_start_run', { ...args });

export const runnerHalt = (session: string): Promise<void> =>
	invoke<void>('runner_halt', { session });

export const runnerOpenTerminal = (session: string): Promise<void> =>
	invoke<void>('runner_open_terminal', { session });

export const runnerWatch = (
	handler: (event: WatchEvent) => void,
): Promise<number> => {
	const onEvent = new Channel<WatchEvent>();
	onEvent.onmessage = handler;
	return invoke<number>('runner_watch', { onEvent });
};

export const runnerUnwatch = (id: number): Promise<void> =>
	invoke<void>('runner_unwatch', { id });

export const runnerTermAttach = (
	handler: (frame: TermFrame) => void,
	cols: number,
	rows: number,
): Promise<TermAttachResult> => {
	const onFrame = new Channel<TermFrame>();
	onFrame.onmessage = handler;
	return invoke<TermAttachResult>('runner_term_attach', {
		onFrame,
		cols,
		rows,
	});
};

export const runnerTermAck = (seq: number): Promise<void> =>
	invoke<void>('runner_term_ack', { seq });

export const runnerTermInput = (data: string): Promise<void> =>
	invoke<void>('runner_term_input', { data });

export const runnerTermResize = (cols: number, rows: number): Promise<void> =>
	invoke<void>('runner_term_resize', { cols, rows });

export const runnerTermScroll = (offset: number): Promise<void> =>
	invoke<void>('runner_term_scroll', { offset });

export const runnerTermDetach = (): Promise<void> =>
	invoke<void>('runner_term_detach');

export const runnerTermOpenLink = (url: string): Promise<void> =>
	invoke<void>('runner_term_open_link', { url });

export interface RunnerAllowedList {
	entries: AllowedEntry[];
	error: string | null;
}

export const runnerAllowedList = (): Promise<RunnerAllowedList> =>
	invoke<RunnerAllowedList>('runner_allowed_list');
