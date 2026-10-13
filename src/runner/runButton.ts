import { isLocalWorkspaceId } from '@/local/classify';
import {
	runnerStartRun,
	type RunnerSession,
	type RunnerStatus,
	type StartRunArgs,
} from './api';

export const CLOUD_TOOLTIP =
	'Agent runs work only with local workspaces for now.';
export const ABSENT_TOOLTIP =
	'TMGR runner is not running. Start it with `tmgr runner start`.';
const INCOMPATIBLE_TOOLTIP =
	'The TMGR runner version is not compatible with this app. Update the CLI.';

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
	if (input.status?.state === 'incompatible') return run(INCOMPATIBLE_TOOLTIP);
	if (input.status?.state !== 'ready') return run(ABSENT_TOOLTIP);
	return run(null);
};

export type StartRunResult =
	| { ok: true; session: RunnerSession }
	| { ok: false; cancelled: true }
	| { ok: false; cancelled: false; error: string };

const errorText = (error: unknown): string => {
	if (typeof error === 'string') return error;
	if (error && typeof (error as any).message === 'string')
		return (error as any).message;
	return String(error);
};

export const startRunForTask = async (
	args: StartRunArgs,
): Promise<StartRunResult> => {
	try {
		return { ok: true, session: await runnerStartRun(args) };
	} catch (error) {
		const text = errorText(error);
		if (text === 'cancelled') return { ok: false, cancelled: true };
		return { ok: false, cancelled: false, error: text };
	}
};
