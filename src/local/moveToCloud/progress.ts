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

const PHASES: Record<string, [RowId, number]> = {
	staging: ['statuses', 100],
	personas: ['statuses', 200],
	statuses: ['statuses', 300],
	categories: ['statuses', 400],
	tasks: ['tasks', 500],
	relations: ['tasks', 600],
	comments: ['comments', 700],
	pages: ['pages', 750],
	page_versions: ['pages', 760],
	routines: ['routines', 780],
	files: ['files', 800],
	assignments: ['routines', 900],
	agent_runs: ['routines', 1000],
	link_rewrite: ['links', 1050],
	modules: ['routines', 1100],
	verify: ['publish', 9000],
};

const ROW_LAST = Object.values(PHASES).reduce<Partial<Record<RowId, number>>>(
	(last, [row, order]) => ({ ...last, [row]: Math.max(last[row] ?? 0, order) }),
	{},
);

export const phaseRow = (phase: string | null | undefined): RowId =>
	PHASES[(phase ?? '').toLowerCase()]?.[0] ?? 'statuses';

const phaseOrder = (phase: string | null | undefined): number =>
	PHASES[(phase ?? '').toLowerCase()]?.[1] ?? 0;

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

	const order = phaseOrder(status?.phase);
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
				index === current
					? 'active'
					: (ROW_LAST[row.id] ?? Infinity) < order
					? 'done'
					: 'pending';
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
