const invoke = jest.fn();
jest.mock('@tauri-apps/api/core', () => ({
	invoke: (...a: unknown[]) => invoke(...a),
}));

import {
	errorText,
	usageConsent,
	usageConsentSet,
	usageForget,
	usageOverview,
	usageScan,
	usageTasks,
} from '../api';

describe('usage api', () => {
	beforeEach(() => invoke.mockReset());

	it('calls the matching commands', async () => {
		invoke.mockResolvedValue({ ok: 1 });
		await usageScan();
		await usageOverview();
		const runs = [{ id: 1 }] as any;
		await usageTasks(runs);
		await usageConsent();
		await usageConsentSet({ state: 'granted', claude: true, codex: false });
		await usageForget();
		expect(invoke.mock.calls).toEqual([
			['usage_scan'],
			['usage_overview'],
			['usage_tasks', { runs }],
			['usage_consent'],
			['usage_consent_set', { state: 'granted', claude: true, codex: false }],
			['usage_forget'],
		]);
	});

	it('extracts readable error text', () => {
		expect(errorText('Logs folder is not readable')).toBe(
			'Logs folder is not readable',
		);
		expect(errorText(new Error('boom'))).toBe('boom');
		expect(errorText(null)).toBe('Something went wrong.');
	});
});
