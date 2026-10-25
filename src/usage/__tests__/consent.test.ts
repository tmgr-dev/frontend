const invoke = jest.fn();
jest.mock('@tauri-apps/api/core', () => ({
	invoke: (...a: unknown[]) => invoke(...a),
}));

import {
	consentGate,
	FORGET_DONE,
	readingNote,
	useUsageConsent,
} from '@/components/usage/consent';
import type { UsageConsent } from '../api';

const consent = (over: Partial<UsageConsent> = {}): UsageConsent => ({
	supported: true,
	unsupported_reason: null,
	state: 'unset',
	claude: true,
	codex: true,
	decided_at: null,
	folders: [{ tool: 'claude', label: 'claude', path: '~/.claude' }],
	has_data: false,
	...over,
});

const commands = () => invoke.mock.calls.map((c) => c[0]);

describe('consentGate', () => {
	it('maps the stored choice to what the AI Usage page shows', () => {
		expect(consentGate(null)).toBeNull();
		expect(consentGate(consent())).toBe('ask');
		expect(consentGate(consent({ state: 'denied' }))).toBe('off');
		expect(consentGate(consent({ state: 'granted' }))).toBe('granted');
		expect(consentGate(consent({ supported: false, state: 'granted' }))).toBe(
			'unsupported',
		);
	});
});

describe('readingNote', () => {
	it('tells the truth about each stored choice', () => {
		expect(readingNote('denied')).toMatch(
			/^Off: TMGR doesn't read these logs for AI usage/,
		);
		expect(readingNote('granted')).toMatch(/^On:/);
		expect(readingNote('unset')).toContain(
			'tmgr usage commands you run yourself still do',
		);
	});
});

describe('useUsageConsent', () => {
	const g = global as { window?: unknown };

	beforeEach(() => {
		invoke.mockReset();
		g.window = { __TAURI_INTERNALS__: {} };
	});
	afterAll(() => {
		delete g.window;
	});

	it('reads the choice without scanning, so the consent screen comes first', async () => {
		invoke.mockResolvedValue(consent());
		const c = useUsageConsent();
		expect(await c.load()).toBe(true);
		expect(c.gate.value).toBe('ask');
		expect(commands()).toEqual(['usage_consent']);
	});

	it('Allow stores the picked tools and returns the stored choice', async () => {
		const stored = consent({ state: 'granted', codex: false });
		invoke.mockResolvedValue(stored);
		const c = useUsageConsent(consent());
		expect(await c.save({ state: 'granted', claude: true, codex: false })).toBe(
			true,
		);
		expect(invoke.mock.calls).toEqual([
			['usage_consent_set', { state: 'granted', claude: true, codex: false }],
		]);
		expect(c.consent.value).toEqual(stored);
		expect(c.gate.value).toBe('granted');
		expect(commands()).not.toContain('usage_scan');
	});

	it('Not now stores denied and shows the off card, no scan', async () => {
		invoke.mockResolvedValue(consent({ state: 'denied' }));
		const c = useUsageConsent(consent());
		await c.save({ state: 'denied' });
		expect(c.gate.value).toBe('off');
		expect(invoke.mock.calls).toEqual([
			['usage_consent_set', { state: 'denied' }],
		]);
	});

	it('a stale page sends only the changed field and shows the stored value', async () => {
		// Cached as granted, but another window denied reading meanwhile.
		const stored = consent({ state: 'denied', claude: false, codex: true });
		invoke.mockResolvedValue(stored);
		const c = useUsageConsent(consent({ state: 'granted' }));
		await c.save({ claude: false });
		expect(invoke.mock.calls).toEqual([
			['usage_consent_set', { claude: false }],
		]);
		expect(c.consent.value).toEqual(stored);
		expect(c.gate.value).toBe('off');
	});

	it('the read switch sends only the state', async () => {
		invoke.mockResolvedValue(consent({ state: 'granted' }));
		const c = useUsageConsent(
			consent({ state: 'denied', claude: false, codex: false }),
		);
		await c.save({ state: 'granted' });
		expect(invoke.mock.calls).toEqual([
			['usage_consent_set', { state: 'granted' }],
		]);
		expect(c.consent.value?.claude).toBe(true);
	});

	it('shows a read error and retries', async () => {
		invoke.mockRejectedValueOnce('Usage data is not readable.');
		const c = useUsageConsent();
		expect(await c.load()).toBe(false);
		expect(c.error.value).toBe('Usage data is not readable.');
		expect(c.consent.value).toBeNull();
		invoke.mockResolvedValueOnce(consent());
		expect(await c.load()).toBe(true);
		expect(c.error.value).toBeNull();
		expect(c.gate.value).toBe('ask');
	});

	it('keeps the old choice when saving fails', async () => {
		const before = consent({ state: 'granted' });
		invoke.mockRejectedValue(new Error('Unknown choice.'));
		const c = useUsageConsent(before);
		expect(await c.save({ state: 'denied' })).toBe(false);
		expect(c.consent.value).toEqual(before);
		expect(c.error.value).toBe('Unknown choice.');
		expect(c.busy.value).toBe(false);
	});

	it('maps a Snap build to unsupported', async () => {
		invoke.mockResolvedValue(
			consent({ supported: false, unsupported_reason: 'Snap', folders: [] }),
		);
		const c = useUsageConsent();
		await c.load();
		expect(c.gate.value).toBe('unsupported');
	});

	it('forget calls usage_forget and reports the result', async () => {
		invoke.mockResolvedValue(consent({ state: 'denied' }));
		const c = useUsageConsent(consent({ state: 'granted', has_data: true }));
		expect(await c.forget()).toBe(true);
		expect(invoke.mock.calls).toEqual([['usage_forget']]);
		expect(c.message.value).toBe(FORGET_DONE);
		expect(c.gate.value).toBe('off');
	});

	it('forget shows the busy error and keeps the choice', async () => {
		invoke.mockRejectedValue('A scan is running. Try again in a moment.');
		const before = consent({ state: 'granted' });
		const c = useUsageConsent(before);
		expect(await c.forget()).toBe(false);
		expect(c.error.value).toBe('A scan is running. Try again in a moment.');
		expect(c.message.value).toBe('');
		expect(c.consent.value).toEqual(before);
	});

	it('forget that fails after turning reading off shows the stored choice', async () => {
		invoke
			.mockRejectedValueOnce('The usage data could not be deleted.')
			.mockResolvedValueOnce(consent({ state: 'denied' }));
		const c = useUsageConsent(consent({ state: 'granted' }));
		expect(await c.forget()).toBe(false);
		expect(commands()).toEqual(['usage_forget', 'usage_consent']);
		expect(c.error.value).toBe('The usage data could not be deleted.');
		expect(c.gate.value).toBe('off');
	});

	it('ignores a second action while one is running', async () => {
		let finish: (v: UsageConsent) => void = () => {};
		invoke.mockReturnValue(new Promise((r) => (finish = r)));
		const c = useUsageConsent(consent({ state: 'granted' }));
		const first = c.forget();
		expect(await c.save({ state: 'denied' })).toBe(false);
		expect(await c.forget()).toBe(false);
		finish(consent({ state: 'denied' }));
		await first;
		expect(commands()).toEqual(['usage_forget']);
	});

	it('does nothing outside the desktop app (web build)', async () => {
		delete g.window;
		const c = useUsageConsent();
		expect(await c.load()).toBe(false);
		expect(c.consent.value).toBeNull();
		expect(invoke).not.toHaveBeenCalled();
	});
});
