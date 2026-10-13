import type { RunnerExitDetail, RunnerState } from './api';

export type StateTone =
	| 'info'
	| 'active'
	| 'neutral'
	| 'attention'
	| 'success'
	| 'danger'
	| 'warning';

export interface StateView {
	label: string;
	tone: StateTone;
	highlight: boolean;
}

const EXIT_DETAILS: RunnerExitDetail[] = ['done', 'failed', 'interrupted'];

const EXITED: Record<RunnerExitDetail, StateView> = {
	done: { label: 'Done', tone: 'success', highlight: false },
	failed: { label: 'Failed', tone: 'danger', highlight: false },
	interrupted: { label: 'Interrupted', tone: 'warning', highlight: false },
};

export const describeState = (state: RunnerState): StateView => {
	switch (state.kind) {
		case 'starting':
			return { label: 'Starting', tone: 'info', highlight: false };
		case 'working':
			return { label: 'Working', tone: 'active', highlight: false };
		case 'waiting':
			return { label: 'Idle', tone: 'neutral', highlight: false };
		case 'blocked':
			return { label: 'Needs input', tone: 'attention', highlight: true };
		case 'exited':
			return { ...EXITED[state.detail ?? 'done'] };
		default:
			return { label: 'Unknown', tone: 'neutral', highlight: false };
	}
};

export const sanitizeState = (state: RunnerState): RunnerState => {
	const detail =
		state.kind === 'exited' &&
		EXIT_DETAILS.includes(state.detail as RunnerExitDetail)
			? state.detail
			: null;
	const clean: RunnerState = {
		kind: state.kind,
		sinceMs: state.sinceMs,
		detail,
	};
	if (detail === 'failed' && Number.isInteger(state.exitCode))
		clean.exitCode = state.exitCode;
	return clean;
};

export const exitCodeLabel = (state: RunnerState): string | null =>
	state.kind === 'exited' &&
	state.detail === 'failed' &&
	Number.isInteger(state.exitCode)
		? `Exited (code ${state.exitCode})`
		: null;

export const TONE_CLASS: Record<StateTone, string> = {
	info: 'bg-sky-100 text-sky-800 dark:bg-sky-500/20 dark:text-sky-200',
	active:
		'bg-emerald-100 text-emerald-800 dark:bg-emerald-500/20 dark:text-emerald-200',
	neutral: 'bg-muted text-muted-foreground',
	attention:
		'bg-amber-100 text-amber-900 ring-2 ring-amber-500 dark:bg-amber-500/25 dark:text-amber-100',
	success:
		'bg-emerald-100 text-emerald-800 dark:bg-emerald-500/20 dark:text-emerald-200',
	danger: 'bg-red-100 text-red-800 dark:bg-red-500/20 dark:text-red-200',
	warning:
		'bg-orange-100 text-orange-800 dark:bg-orange-500/20 dark:text-orange-200',
};
