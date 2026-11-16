export type SlackRedirect =
	| { kind: 'confirm'; flow: 'install' | 'link'; pending: string }
	| { kind: 'error'; reason: string };

type Query = Record<string, unknown>;

const first = (value: unknown): string | null => {
	const item = Array.isArray(value) ? value[0] : value;
	return typeof item === 'string' && item ? item : null;
};

const REASONS = [
	'invalid_state',
	'access_denied',
	'forbidden',
	'module_disabled',
	'slack_not_installed',
	'slack_team_mismatch',
	'slack_guest_not_allowed',
	'slack_user_inactive',
	'slack_user_linked_elsewhere',
	'invalid_id_token',
	'slack_unavailable',
	'slack_api_error',
	'slack_token_rotation_required',
	'server_error',
];

export const parseSlackRedirect = (query: Query): SlackRedirect | null => {
	const status = first(query.status);
	if (status === 'confirm') {
		const flow = first(query.flow);
		const pending = first(query.pending);
		if ((flow === 'install' || flow === 'link') && pending) {
			return { kind: 'confirm', flow, pending };
		}
		return null;
	}
	if (status === 'error') {
		return { kind: 'error', reason: first(query.reason) ?? 'unknown' };
	}
	return null;
};

export const slackReasonKey = (reason: string): string =>
	`settings.slack.errors.${REASONS.includes(reason) ? reason : 'unknown'}`;

export const slackConfirmFailureKey = (error: unknown): string => {
	const reason = (error as { response?: { data?: { error?: unknown } } })
		?.response?.data?.error;
	if (reason === 'invalid_state') return 'settings.slack.errors.expired';
	return slackReasonKey(typeof reason === 'string' ? reason : 'unknown');
};
