export type RunnerAction = 'start' | 'stop' | 'input' | 'open';

export interface RunnerErrorView {
	cancelled: boolean;
	text: string;
}

const MAX_LENGTH = 160;

const FIXED: Array<[RegExp, string]> = [
	[/^busy\b/, 'The agent is busy, try again in a moment.'],
	[
		/^unknown_profile\b/,
		'This agent profile is not available. Pick another profile.',
	],
	[
		/^sandbox\b/,
		'The sandbox blocked this run. Check the project folder and try again.',
	],
	[/^limit\b/, 'Too many agents are running. Stop one and try again.'],
	[
		/^invalid_params\b/,
		'The runner rejected the request. Check the project folder and try again.',
	],
	[/^task not found\b/, 'This task was not found in the local workspace.'],
];

const PREFIX: Record<RunnerAction, string> = {
	start: 'Could not start the agent',
	stop: 'Could not stop the agent',
	input: 'Could not send input to the agent',
	open: 'Could not open the terminal',
};

export const rawErrorText = (error: unknown): string => {
	if (typeof error === 'string') return error;
	if (error && typeof (error as any).message === 'string')
		return (error as any).message;
	return String(error);
};

export const sanitizeMessage = (text: string): string => {
	const flat = text
		.replace(/[\u0000-\u001f\u007f-\u009f]+/g, ' ')
		.replace(/\s+/g, ' ')
		.trim();
	return flat.length > MAX_LENGTH ? `${flat.slice(0, MAX_LENGTH)}…` : flat;
};

export const describeRunnerError = (
	error: unknown,
	action: RunnerAction = 'start',
): RunnerErrorView => {
	const raw = rawErrorText(error);
	const key = raw.trim().toLowerCase();
	if (key === 'cancelled') return { cancelled: true, text: '' };
	for (const [pattern, text] of FIXED)
		if (pattern.test(key)) return { cancelled: false, text };
	const message = sanitizeMessage(raw);
	return {
		cancelled: false,
		text: message ? `${PREFIX[action]}: ${message}` : `${PREFIX[action]}.`,
	};
};
