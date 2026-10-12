/** @jest-environment jsdom */
import { CONSENT_KEY, createConsentStore } from '@/privacy/consent';
import { QUEUE_KEY } from '../client';
import { IDENTITY_KEY, RECEIPTS_KEY } from '../identity';
import { FakeChannel, FlakyStorage, makeClock, makeTab, setConsent, tick } from './harness';

beforeEach(() => {
	localStorage.clear();
	FakeChannel.all = [];
});
afterEach(tick);

const stored = (key: string) => JSON.parse(localStorage.getItem(key) ?? 'null');
const bodyOf = (call: { init: RequestInit }) => JSON.parse(call.init.body as string);

describe('a denial that cannot be persisted', () => {
	const setup = () => {
		const storage = new FlakyStorage();
		const win = { localStorage: storage, addEventListener: () => {} } as unknown as Window;
		const store = createConsentStore(win);
		store.set({ usage: 'granted' });
		const tab = makeTab({
			storage: storage as unknown as Storage,
			consent: {
				read: () => {
					const r = store.read();
					return { on: r.usage === 'granted', generation: r.usageGeneration };
				},
				deny: () => void store.set({ usage: 'denied' }),
			},
		});
		tab.telemetry.sync();
		tab.telemetry.track('app_opened');
		return { storage, store, tab };
	};

	it('stops sending in this window when consent cannot be written or removed', async () => {
		const { storage, store, tab } = setup();
		storage.failSet = (key) => key === CONSENT_KEY;
		storage.failRemove = true;
		store.set({ usage: 'denied' });
		expect(JSON.parse(storage.data.get(CONSENT_KEY)!).usage).toBe('granted');
		await tab.telemetry.flush({ keepalive: true });
		expect(tab.calls).toEqual([]);
		expect(tab.telemetry.snapshot().active).toBe(false);
	});

	it('stops sending when removal works after the write failed', async () => {
		const { storage, store, tab } = setup();
		storage.failSet = (key) => key === CONSENT_KEY;
		store.set({ usage: 'denied' });
		await tab.telemetry.flush({ keepalive: true });
		expect(tab.calls).toEqual([]);
	});

	it('survives storage that throws while stopping', async () => {
		const { storage, store, tab } = setup();
		storage.failSet = () => true;
		storage.failRemove = true;
		expect(() => store.set({ usage: 'denied' })).not.toThrow();
		expect(() => tab.telemetry.sync()).not.toThrow();
	});
});

describe('queue writes racing a withdrawal in another tab', () => {
	it('a pending write in tab A cannot restore the queue tab B cleared', async () => {
		setConsent('granted', 1);
		const a = makeTab();
		const b = makeTab();
		a.telemetry.sync();
		b.telemetry.sync();
		a.telemetry.track('app_opened');
		setConsent('denied', 2);
		b.telemetry.sync();
		await tick();
		await tick();
		expect(localStorage.getItem(QUEUE_KEY)).toBeNull();
	});

	it('writes under the queue lock and re-checks consent inside it', async () => {
		setConsent('granted', 1);
		const names: string[] = [];
		const held: Array<() => unknown> = [];
		const tab = makeTab({
			locks: {
				request: (name: string, cb: () => unknown) => {
					names.push(name);
					held.push(cb);
					return Promise.resolve();
				},
			},
		});
		tab.telemetry.sync();
		tab.telemetry.track('app_opened');
		await tick();
		expect(names).toContain('tmgr-telemetry-queue');
		setConsent('denied', 2);
		held.forEach((cb) => cb());
		expect(localStorage.getItem(QUEUE_KEY)).toBeNull();
	});
});

describe('deletion credentials', () => {
	it('keeps the identity when the receipt cannot be saved and still deletes with it', async () => {
		const storage = new FlakyStorage();
		setConsent('granted', 1);
		const consent = {
			read: () => {
				const raw = JSON.parse(localStorage.getItem(CONSENT_KEY) ?? '{}');
				return { on: raw.usage === 'granted', generation: raw.usageGeneration ?? 0 };
			},
			deny: () => setConsent('denied', 2),
		};
		const tab = makeTab({ storage: storage as unknown as Storage, consent });
		tab.telemetry.sync();
		const id = JSON.parse(storage.data.get(IDENTITY_KEY)!);
		storage.failSet = (key) => key === RECEIPTS_KEY;
		setConsent('denied', 2);
		tab.telemetry.sync();
		expect(storage.data.has(IDENTITY_KEY)).toBe(true);
		expect(tab.telemetry.snapshot().active).toBe(false);
		expect(tab.telemetry.snapshot().error).toBe('receipt');
		tab.calls.length = 0;
		const result = await tab.telemetry.deleteSent();
		expect(result).toEqual({ deleted: 1, failed: 0 });
		expect(tab.calls[0].url).toContain(id.install_id);
		expect(storage.data.has(IDENTITY_KEY)).toBe(false);
	});

	it('removes the identity once the receipt is durable', () => {
		setConsent('granted', 1);
		const tab = makeTab();
		tab.telemetry.sync();
		setConsent('denied', 2);
		tab.telemetry.sync();
		expect(localStorage.getItem(IDENTITY_KEY)).toBeNull();
		expect(stored(RECEIPTS_KEY)).toHaveLength(1);
		expect(tab.telemetry.snapshot().error).toBeNull();
	});

	it('prunes expired receipts at startup and persists the result', () => {
		const clock = makeClock();
		localStorage.setItem(
			RECEIPTS_KEY,
			JSON.stringify([
				{ install_id: 'old', secret: 's', created: clock.now() - 31 * 24 * 3600_000 },
				{ install_id: 'new', secret: 's', created: clock.now() - 1000 },
			]),
		);
		makeTab({}, clock);
		expect(stored(RECEIPTS_KEY).map((r: { install_id: string }) => r.install_id)).toEqual(['new']);
	});
});

describe('what leaves the device', () => {
	beforeEach(() => setConsent('granted', 1));

	it('copies props at enqueue so later mutation cannot change the payload', async () => {
		const tab = makeTab();
		const props = { route: 'Board' as const };
		tab.telemetry.track('screen_viewed', props);
		(props as Record<string, unknown>).route = 'Evil';
		(props as Record<string, unknown>).email = 'a@b.c';
		await tab.telemetry.flush();
		expect(bodyOf(tab.calls[0]).events[0].p).toEqual({ route: 'Board' });
	});

	it('reduces app_version to major.minor whatever the source says', async () => {
		const tab = makeTab({
			meta: () => ({ platform: 'web', app_version: '3.4.87-rc1', account_mode: 'cloud', viewport: 'l' }),
		});
		tab.telemetry.track('app_opened');
		await tab.telemetry.flush();
		expect(bodyOf(tab.calls[0]).app_version).toBe('3.4');
	});

	it('drops poisoned entries from a restored queue', async () => {
		const m = {
			session_id: '00000000-0000-4000-8000-000000000001',
			platform: 'web',
			app_version: '3.4',
			account_mode: 'cloud',
			viewport: 'l',
		};
		const good = { id: '00000000-0000-4000-8000-0000000000aa', n: 'app_opened', t: 1791763200, seq: 0, p: {}, m };
		const bad = [
			{ ...good, id: '00000000-0000-4000-8000-0000000000b1', t: 1791763211 },
			{ ...good, id: '00000000-0000-4000-8000-0000000000b2', n: 'nope' },
			{ ...good, id: '00000000-0000-4000-8000-0000000000b3', n: 'screen_viewed', p: { route: 'Secret' } },
			{ ...good, id: '00000000-0000-4000-8000-0000000000b4', p: { email: 'a@b.c' } },
			{ ...good, id: '00000000-0000-4000-8000-0000000000b5', m: { ...m, app_version: '3.4.87' } },
			{ ...good, id: 'not-a-uuid' },
			{ ...good, id: '00000000-0000-4000-8000-0000000000b6', seq: -1 },
			{ ...good, id: '00000000-0000-4000-8000-0000000000b7', t: 'x' },
		];
		const clock = makeClock();
		const goodT = Math.floor(clock.now() / 3_600_000) * 3600;
		localStorage.setItem(
			QUEUE_KEY,
			JSON.stringify({
				generation: 1,
				events: [{ ...good, t: goodT, extra: 'x' }, ...bad.map((e) => ({ ...e, t: e.t === 1791763211 ? goodT + 11 : e.t === 1791763200 ? goodT : e.t }))],
			}),
		);
		const tab = makeTab({}, clock);
		tab.telemetry.sync();
		expect(tab.telemetry.snapshot().pending).toBe(1);
		await tab.telemetry.flush();
		const event = bodyOf(tab.calls[0]).events[0];
		expect(Object.keys(event).sort()).toEqual(['id', 'n', 'p', 'seq', 't']);
	});

	it('builds the envelope from exactly the allowed keys', async () => {
		const tab = makeTab();
		tab.telemetry.track('app_opened');
		await tab.telemetry.flush();
		expect(Object.keys(bodyOf(tab.calls[0])).sort()).toEqual([
			'account_mode',
			'app_version',
			'events',
			'install_id',
			'platform',
			'schema',
			'secret_sha256',
			'session_id',
			'viewport',
		]);
	});
});
