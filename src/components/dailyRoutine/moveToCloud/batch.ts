import type { LocalRoutine } from './payload';

export type MoveMode = 'routines' | 'tasks';

export interface MoveTarget {
	workspaceId: number;
	categoryId: number | null;
	statusId: number | null;
}

export type CloudErrorKind = 'auth' | 'network' | 'rejected';

export class CloudError extends Error {
	constructor(
		readonly kind: CloudErrorKind,
		message: string,
	) {
		super(message);
		this.name = 'CloudError';
	}
}

export type ItemStatus = 'moved' | 'failed' | 'in_cloud' | 'not_sent';

export interface ItemResult {
	id: number;
	title: string;
	status: ItemStatus;
	key: string;
	message?: string;
	cloudId?: number | null;
}

export interface MoveDeps {
	create(
		routine: LocalRoutine,
		mode: MoveMode,
		target: MoveTarget,
		key: string,
	): Promise<{ id: number | null }>;
	removeLocal(id: number): Promise<void>;
}

export interface MoveOutcome {
	results: ItemResult[];
	stopped: 'auth' | 'network' | null;
}

export const STOP_MESSAGE = {
	auth: 'Your session expired, so the remaining routines were not sent. Sign in again and retry them.',
	network:
		'The TMGR server could not be reached, so the remaining routines were not sent. Check your connection and retry them.',
} as const;

const MAYBE_SENT_MESSAGE =
	'May have reached the server. Check the workspace before retrying.';

let counter = 0;
const newKey = (): string => {
	const random =
		typeof crypto !== 'undefined' && 'randomUUID' in crypto
			? crypto.randomUUID()
			: `${Date.now().toString(36)}-${(counter++).toString(36)}`;
	return `routine-move-${random}`;
};

export const toMessage = (error: unknown): string =>
	(error as Error)?.message || 'Something went wrong.';

const pending = (routine: LocalRoutine, prior?: ItemResult): ItemResult => ({
	id: routine.id,
	title: routine.title,
	status: 'not_sent',
	key: prior?.key ?? newKey(),
});

const process = async (
	routines: LocalRoutine[],
	previous: Map<number, ItemResult>,
	redo: Set<number>,
	mode: MoveMode,
	target: MoveTarget,
	deps: MoveDeps,
): Promise<MoveOutcome> => {
	let stopped: MoveOutcome['stopped'] = null;
	const results: ItemResult[] = [];
	for (const routine of routines) {
		const prior = previous.get(routine.id);
		if (prior && !redo.has(routine.id)) {
			results.push(prior);
			continue;
		}
		const item = pending(routine, prior);
		results.push(item);
		if (stopped) {
			item.message = STOP_MESSAGE[stopped];
			continue;
		}
		let cloudId = prior?.status === 'in_cloud' ? (prior.cloudId ?? null) : null;
		if (prior?.status !== 'in_cloud') {
			try {
				cloudId = (await deps.create(routine, mode, target, item.key)).id;
			} catch (error) {
				item.status = 'failed';
				item.message =
					mode === 'routines' &&
					error instanceof CloudError &&
					error.kind === 'network'
						? MAYBE_SENT_MESSAGE
						: toMessage(error);
				if (error instanceof CloudError && error.kind !== 'rejected')
					stopped = error.kind;
				continue;
			}
		}
		item.cloudId = cloudId;
		try {
			await deps.removeLocal(routine.id);
			item.status = 'moved';
		} catch (error) {
			item.status = 'in_cloud';
			item.message = toMessage(error);
		}
	}
	return { results, stopped };
};

export const runMove = (
	routines: LocalRoutine[],
	mode: MoveMode,
	target: MoveTarget,
	deps: MoveDeps,
): Promise<MoveOutcome> =>
	process(
		routines,
		new Map(),
		new Set(routines.map((r) => r.id)),
		mode,
		target,
		deps,
	);

export const retryMove = (
	routines: LocalRoutine[],
	previous: ItemResult[],
	ids: number[],
	mode: MoveMode,
	target: MoveTarget,
	deps: MoveDeps,
): Promise<MoveOutcome> =>
	process(
		routines,
		new Map(previous.map((item) => [item.id, item])),
		new Set(ids),
		mode,
		target,
		deps,
	);
