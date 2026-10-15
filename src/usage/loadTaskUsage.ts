import type { RawRun } from './api';
import { usageScan } from './api';
import { fetchRuns, toRawRun } from './runs';
import { usageTask, type TaskUsage } from './taskUsage';

const RUNS_TTL_MS = 60_000;

const cache = new Map<number, { at: number; runs: Promise<RawRun[] | null> }>();

export const resetRunsCache = (): void => cache.clear();

const workspaceRuns = (
	workspaceId: number,
	workspaceCode: string,
	now: number,
): Promise<RawRun[] | null> => {
	const known = cache.get(workspaceId);
	if (known && now - known.at < RUNS_TTL_MS) return known.runs;
	const runs = fetchRuns([{ id: workspaceId, code: workspaceCode }]).then(
		(fetched) => (fetched.failedWorkspaces ? null : fetched.runs),
	);
	cache.set(workspaceId, { at: now, runs });
	return runs;
};

export interface TaskUsageRequest {
	taskId: number;
	workspaceId?: number;
	workspaceCode: string;
	ownRuns: any[];
}

export const collectRuns = async (
	req: TaskUsageRequest,
	now: number = Date.now(),
): Promise<RawRun[]> => {
	const own = req.ownRuns.map((run) => toRawRun(run, req.workspaceCode));
	if (req.workspaceId == null || !(req.workspaceId >= 0)) return own;
	const all = await workspaceRuns(req.workspaceId, req.workspaceCode, now);
	return all ?? own;
};

export const readTaskUsage = async (
	req: TaskUsageRequest,
	runs?: RawRun[],
): Promise<{ usage: TaskUsage; runs: RawRun[] }> => {
	const used = runs ?? (await collectRuns(req));
	const usage = await usageTask({
		runs: used,
		workspace: req.workspaceCode,
		task_id: req.taskId,
	});
	return { usage, runs: used };
};

export const scanThenRead = async (
	req: TaskUsageRequest,
	runs: RawRun[],
): Promise<TaskUsage | null> => {
	const scan = await usageScan();
	if (scan.state !== 'done' || !(scan.events_new || scan.events_updated))
		return null;
	return (await readTaskUsage(req, runs)).usage;
};
