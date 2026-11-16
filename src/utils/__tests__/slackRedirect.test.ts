import {
	parseSlackRedirect,
	slackConfirmFailureKey,
	slackReasonKey,
} from '../slackRedirect';

describe('parseSlackRedirect', () => {
	it('parses an install confirm', () => {
		expect(
			parseSlackRedirect({ status: 'confirm', flow: 'install', pending: 'p1' }),
		).toEqual({ kind: 'confirm', flow: 'install', pending: 'p1' });
	});

	it('parses a link confirm', () => {
		expect(
			parseSlackRedirect({ status: 'confirm', flow: 'link', pending: 'p2' }),
		).toEqual({ kind: 'confirm', flow: 'link', pending: 'p2' });
	});

	it('rejects a confirm without a pending id or with an unknown flow', () => {
		expect(parseSlackRedirect({ status: 'confirm', flow: 'link' })).toBeNull();
		expect(
			parseSlackRedirect({ status: 'confirm', flow: 'x', pending: 'p' }),
		).toBeNull();
	});

	it('parses an error reason', () => {
		expect(
			parseSlackRedirect({ status: 'error', reason: 'access_denied' }),
		).toEqual({ kind: 'error', reason: 'access_denied' });
	});

	it('treats an error without a reason as unknown', () => {
		expect(parseSlackRedirect({ status: 'error' })).toEqual({
			kind: 'error',
			reason: 'unknown',
		});
	});

	it('returns null for a plain visit and takes the first array value', () => {
		expect(parseSlackRedirect({})).toBeNull();
		expect(parseSlackRedirect({ status: 'other' })).toBeNull();
		expect(
			parseSlackRedirect({
				status: ['confirm'],
				flow: ['link'],
				pending: ['p3'],
			}),
		).toEqual({ kind: 'confirm', flow: 'link', pending: 'p3' });
	});
});

describe('slackReasonKey', () => {
	it.each([
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
	])('maps %s to its own message', (reason) => {
		expect(slackReasonKey(reason)).toBe(`settings.slack.errors.${reason}`);
	});

	it('falls back to the generic message', () => {
		expect(slackReasonKey('whatever')).toBe('settings.slack.errors.unknown');
		expect(slackReasonKey('unknown')).toBe('settings.slack.errors.unknown');
	});
});

describe('slackConfirmFailureKey', () => {
	it('maps invalid_state to already done or expired', () => {
		expect(
			slackConfirmFailureKey({
				response: { data: { error: 'invalid_state' } },
			}),
		).toBe('settings.slack.errors.expired');
	});

	it('maps a known reason and falls back for anything else', () => {
		expect(
			slackConfirmFailureKey({
				response: { data: { error: 'slack_team_mismatch' } },
			}),
		).toBe('settings.slack.errors.slack_team_mismatch');
		expect(slackConfirmFailureKey(new Error('boom'))).toBe(
			'settings.slack.errors.unknown',
		);
	});
});
