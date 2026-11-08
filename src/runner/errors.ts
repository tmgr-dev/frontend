import { t } from '@/i18n';

export type RunnerAction = 'start' | 'stop' | 'input' | 'open';

export interface RunnerErrorView {
	cancelled: boolean;
	text: string;
}

const MAX_LENGTH = 160;

const FIXED: Array<[RegExp, string]> = [
	[/^busy\b/, 'busy'],
	[/^unknown_profile\b/, 'unknownProfile'],
	[/^sandbox\b/, 'sandbox'],
	[/^limit\b/, 'limit'],
	[/^invalid_params\b/, 'invalidParams'],
	[/^task not found\b/, 'taskNotFound'],
];

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
	for (const [pattern, name] of FIXED)
		if (pattern.test(key))
			return {
				cancelled: false,
				text: t(`desktop.helpers.runner.errors.${name}`),
			};
	const message = sanitizeMessage(raw);
	const prefix = t(`desktop.helpers.runner.errors.prefix.${action}`);
	return {
		cancelled: false,
		text: message
			? t('desktop.helpers.runner.errors.withMessage', { prefix, message })
			: t('desktop.helpers.runner.errors.withoutMessage', { prefix }),
	};
};
