import type { MoveView } from './flow';
import { stepIndex } from './model';

export type RowState = 'done' | 'active' | 'pending' | 'warn';

export interface ProgressRow {
	id: string;
	label: string;
	state: RowState;
}

export interface ProgressSummary {
	percent: number;
	step: number;
	steps: number;
	rows: ProgressRow[];
}

type RowId =
	| 'backup'
	| 'bundle'
	| 'statuses'
	| 'tasks'
	| 'comments'
	| 'files'
	| 'pages'
	| 'links'
	| 'routines'
	| 'publish';

const ROWS: { id: RowId; label: string }[] = [
	{ id: 'backup', label: 'Backup created' },
	{ id: 'bundle', label: 'Bundle uploaded' },
	{ id: 'statuses', label: 'Statuses and categories' },
	{ id: 'tasks', label: 'Tasks' },
	{ id: 'comments', label: 'Comments' },
	{ id: 'files', label: 'Files' },
	{ id: 'pages', label: 'Pages' },
	{ id: 'links', label: 'Links' },
	{ id: 'routines', label: 'Routines and agent runs' },
	{ id: 'publish', label: 'Check and publish' },
];

const PHASE_ROWS: [string, RowId][] = [
	['valid', 'statuses'],
	['stag', 'statuses'],
	['persona', 'statuses'],
	['status', 'statuses'],
	['categor', 'statuses'],
	['task', 'tasks'],
	['relation', 'tasks'],
	['comment', 'comments'],
	['file', 'files'],
	['page', 'pages'],
	['version', 'pages'],
	['link', 'links'],
	['routine', 'routines'],
	['agent', 'routines'],
	['assign', 'routines'],
	['module', 'routines'],
	['verif', 'publish'],
	['publish', 'publish'],
];

export const phaseRow = (phase: string | null | undefined): RowId => {
	const normalized = (phase ?? '').toLowerCase();
	return (
		PHASE_ROWS.find(([prefix]) => normalized.startsWith(prefix))?.[1] ??
		'statuses'
	);
};

const countLabel = (label: string, done: number, total: number) =>
	total > 0 ? `${label} ${done} of ${total}` : label;

export const summarize = (view: MoveView): ProgressSummary => {
	const job = view.job;
	const status = view.status;
	const step = job ? stepIndex(job.step) : stepIndex('published');
	const skipped = job?.excluded.length ?? 0;
	const serverActive =
		!!status && (status.status === 'running' || status.status === 'failed');
	const serverDone =
		status?.status === 'verified' || status?.status === 'published';
	const current = serverActive
		? ROWS.findIndex((r) => r.id === phaseRow(status!.phase))
		: -1;

	const rows: ProgressRow[] = ROWS.map((row, index) => {
		let state: RowState = 'pending';
		if (row.id === 'backup')
			state = step >= stepIndex('snapshotted') ? 'done' : 'active';
		else if (row.id === 'bundle') {
			state =
				step >= stepIndex('bundleUploaded')
					? 'done'
					: step >= stepIndex('snapshotted')
					? 'active'
					: 'pending';
		} else if (serverDone || !job) state = 'done';
		else if (current >= 0) {
			state =
				index < current ? 'done' : index === current ? 'active' : 'pending';
		}
		if (
			row.id === 'files' &&
			state === 'pending' &&
			step >= stepIndex('bundleUploaded') &&
			step < stepIndex('filesUploaded')
		) {
			state = 'active';
		}
		if (row.id === 'files' && skipped > 0 && state === 'done') state = 'warn';
		let label = row.label;
		if (row.id === 'files') {
			const serverRow = current === index && status ? status : null;
			const total = serverRow ? serverRow.total : view.filesTotal;
			const done = serverRow ? serverRow.done : view.filesDone;
			label = countLabel(label, done, total);
			if (skipped > 0) label += ` · ${skipped} skipped (see report)`;
		} else if (current === index && status && status.total > 0) {
			label = countLabel(label, status.done, status.total);
		}
		return { id: row.id, label, state };
	});

	const done = rows.filter(
		(r) => r.state === 'done' || r.state === 'warn',
	).length;
	const active = rows.find((r) => r.state === 'active');
	let fraction = 0;
	if (
		active &&
		active.id !== 'files' &&
		status &&
		current >= 0 &&
		status.total > 0
	) {
		fraction = Math.min(1, status.done / status.total);
	} else if (active?.id === 'files' && view.filesTotal > 0 && !serverActive) {
		fraction = Math.min(1, view.filesDone / view.filesTotal);
	}
	const percent = Math.min(
		100,
		Math.round(((done + fraction) / ROWS.length) * 100),
	);
	return {
		percent,
		step: Math.min(ROWS.length, done + 1),
		steps: ROWS.length,
		rows,
	};
};
