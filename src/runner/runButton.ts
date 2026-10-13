import { isLocalWorkspaceId } from '@/local/classify';
import {
	runnerStartRun,
	type RunnerSession,
	type RunnerStatus,
	type StartRunArgs,
} from './api';
import { describeRunnerError } from './errors';
import { statusDetail } from './statusLine';

export const CLOUD_TOOLTIP =
	'Agent runs work only with local workspaces for now.';
export const ABSENT_TOOLTIP =
	'TMGR runner is not running. Start it with `tmgr runner start`.';

export interface RunButtonModel {
	visible: boolean;
	mode: 'run' | 'open';
	disabled: boolean;
	tooltip: string | null;
}

export const runButtonModel = (input: {
	supported: boolean;
	enabled: boolean;
	status: RunnerStatus | null;
	workspaceId: number | null;
	hasSession: boolean;
}): RunButtonModel => {
	const hidden = { visible: false, mode: 'run', disabled: true, tooltip: null };
	if (!input.supported || !input.enabled) return hidden as RunButtonModel;
	if (input.hasSession)
		return { visible: true, mode: 'open', disabled: false, tooltip: null };
	const run = (tooltip: string | null): RunButtonModel => ({
		visible: true,
		mode: 'run',
		disabled: tooltip !== null,
		tooltip,
	});
	if (input.workspaceId === null || !isLocalWorkspaceId(input.workspaceId))
		return run(CLOUD_TOOLTIP);
	if (input.status?.state === 'incompatible')
		return run(statusDetail(input.status));
	if (input.status?.state !== 'ready') return run(ABSENT_TOOLTIP);
	return run(null);
};

export type StartRunResult =
	| { ok: true; session: RunnerSession }
	| { ok: false; cancelled: true }
	| { ok: false; cancelled: false; error: string };

export const startRunForTask = async (
	args: StartRunArgs,
): Promise<StartRunResult> => {
	try {
		return { ok: true, session: await runnerStartRun(args) };
	} catch (error) {
		const view = describeRunnerError(error, 'start');
		if (view.cancelled) return { ok: false, cancelled: true };
		return { ok: false, cancelled: false, error: view.text };
	}
};
