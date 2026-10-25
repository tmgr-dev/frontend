const invoke = jest.fn();
jest.mock('@tauri-apps/api/core', () => ({
	invoke: (...a: unknown[]) => invoke(...a),
}));

import {
	consentGate,
	FORGET_DONE,
	nextConsent,
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
		expect(readingNote('denied')).toMatch(/^Off: TMGR doesn't open/);
		expect(readingNote('granted')).toMatch(/^On:/);
		expect(readingNote('unset')).toContain(
			'tmgr usage commands you run yourself still do',
		);
	});
});

describe('nextConsent', () => {
	it('keeps the tool flags when reading is switched', () => {
		const c = consent({ state: 'granted', codex: false });
		expect(nextConsent(c, { state: 'denied' })).toEqual({
			state: 'denied',
			claude: true,
			codex: false,
		});
	});

	it('turns both tools on when reading is switched on with both off', () => {
		const c = consent({ state: 'denied', claude: false, codex: false });
		expect(nextConsent(c, { state: 'granted' })).toEqual({
			state: 'granted',
			claude: true,
			codex: true,
		});
	});

	it('sends explicit tool choices as they are', () => {
		const c = consent({ state: 'granted' });
		expect(nextConsent(c, { codex: false })).toEqual({
			state: 'granted',
			claude: true,
			codex: false,
		});
		expect(
			nextConsent(consent(), { state: 'denied', claude: false, codex: true }),
		).toEqual({ state: 'denied', claude: false, codex: true });
	});

	it('does not grant from an unset choice when only a tool changes', () => {
		expect(nextConsent(consent(), { claude: false }).state).toBe('denied');
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
		await c.save({ state: 'denied', claude: true, codex: true });
		expect(c.gate.value).toBe('off');
		expect(commands()).toEqual(['usage_consent_set']);
	});

	it('renders what the backend stored, not what was sent', async () => {
		invoke.mockResolvedValue(
			consent({ state: 'denied', claude: false, codex: false }),
		);
		const c = useUsageConsent(consent({ state: 'granted', codex: false }));
		await c.save({ claude: false });
		expect(invoke.mock.calls[0][1]).toEqual({
			state: 'granted',
			claude: false,
			codex: false,
		});
		expect(c.gate.value).toBe('off');
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
