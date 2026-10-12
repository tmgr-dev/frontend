/** @jest-environment jsdom */
import { QUEUE_KEY } from '../client';
import { createUsageConsent, USAGE_KEY } from '../consent';
import { createIdentityStore, IDENTITY_KEY, RECEIPT_PREFIX } from '../identity';
import { FakeChannel, FlakyStorage, makeClock, makeTab, receiptKeys, setConsent, tick } from './harness';

beforeEach(() => {
	localStorage.clear();
	FakeChannel.all = [];
});
afterEach(tick);

const stored = (key: string) => JSON.parse(localStorage.getItem(key) ?? 'null');
const bodyOf = (call: { init: RequestInit }) => JSON.parse(call.init.body as string);
const ids = {
	a: '00000000-0000-4000-8000-0000000000aa',
	b: '00000000-0000-4000-8000-0000000000bb',
};
const META = {
	session_id: '00000000-0000-4000-8000-000000000001',
	platform: 'web',
	app_version: '3.4',
	account_mode: 'cloud',
	viewport: 'l',
};
const entry = (over: Record<string, unknown> = {}) => ({
	id: ids.a,
	n: 'app_opened',
	t: Math.floor(Date.UTC(2026, 9, 12, 10) / 1000 / 3600) * 3600,
	seq: 0,
	p: {},
	m: META,
	g: 1,
	...over,
});

describe('a denial that cannot be persisted', () => {
	const setup = () => {
		const storage = new FlakyStorage();
		const win = { localStorage: storage, addEventListener: () => {} } as unknown as Window;
		const usage = createUsageConsent(win);
		usage.set(true);
		const tab = makeTab({
			storage: storage as unknown as Storage,
			consent: {
				read: () => {
					const { on, generation } = usage.read();
					return { on, generation };
				},
				deny: () => void usage.set(false),
			},
		});
		tab.telemetry.sync();
		tab.telemetry.track('app_opened');
		return { storage, usage, tab };
	};

	it('stops sending in this window when consent can be neither written nor removed', async () => {
		const { storage, usage, tab } = setup();
		storage.failSet = () => true;
		storage.failRemove = true;
		usage.set(false);
		expect(JSON.parse(storage.data.get(USAGE_KEY)!).on).toBe(true);
		tab.telemetry.sync();
		await tab.telemetry.flush({ keepalive: true });
		expect(tab.calls).toEqual([]);
		expect(tab.telemetry.snapshot().active).toBe(false);
	});

	it('survives storage that throws while stopping', () => {
		const { storage, usage, tab } = setup();
		storage.failSet = () => true;
		storage.failRemove = true;
		expect(() => usage.set(false)).not.toThrow();
		expect(() => tab.telemetry.sync()).not.toThrow();
	});
});

describe('receipts as one key per install', () => {
	const store = (storage: Storage, clock = makeClock()) =>
		createIdentityStore({ storage, now: clock.now, uuid: () => ids.b, secret: () => 's'.repeat(64) });

	it('writes {secret, expiresAt} under its own key', () => {
		setConsent('granted', 1);
		const tab = makeTab();
		tab.telemetry.sync();
		const id = stored(IDENTITY_KEY);
		setConsent('denied', 2);
		tab.telemetry.sync();
		expect(stored(RECEIPT_PREFIX + id.install_id)).toEqual({
			secret: id.secret,
			expiresAt: tab.clock.now() + 30 * 24 * 3600_000,
		});
	});

	it('pruning in one tab keeps a receipt another tab just wrote', () => {
		const clock = makeClock();
		localStorage.setItem(
			RECEIPT_PREFIX + ids.a,
			JSON.stringify({ secret: 'x', expiresAt: clock.now() - 1 }),
		);
		const a = store(localStorage, clock);
		const b = store(localStorage, clock);
		localStorage.setItem(IDENTITY_KEY, JSON.stringify({ install_id: ids.b, secret: 'y', registered: false }));
		expect(b.retire()).toBe(true);
		expect(a.receipts().map((r) => r.install_id)).toEqual([ids.b]);
		expect(receiptKeys()).toEqual([RECEIPT_PREFIX + ids.b]);
	});

	it('writing a receipt never rewrites other receipts', () => {
		const writes: string[] = [];
		const storage = new FlakyStorage();
		const spy = {
			get length() {
				return storage.length;
			},
			key: (i: number) => storage.key(i),
			getItem: (k: string) => storage.getItem(k),
			setItem: (k: string, v: string) => (writes.push(k), storage.setItem(k, v)),
			removeItem: (k: string) => storage.removeItem(k),
		} as unknown as Storage;
		storage.data.set(RECEIPT_PREFIX + ids.a, JSON.stringify({ secret: 'x', expiresAt: Date.now() + 1e9 }));
		storage.data.set(IDENTITY_KEY, JSON.stringify({ install_id: ids.b, secret: 'y', registered: false }));
		store(spy, makeClock(Date.now())).retire();
		expect(writes).toEqual([RECEIPT_PREFIX + ids.b]);
	});

	it('retire never deletes a newer identity created by another tab', () => {
		const storage = new FlakyStorage();
		storage.data.set(IDENTITY_KEY, JSON.stringify({ install_id: ids.a, secret: 'x', registered: false }));
		const original = storage.setItem.bind(storage);
		storage.setItem = (key: string, value: string) => {
			original(key, value);
			if (key === RECEIPT_PREFIX + ids.a) {
				storage.data.set(IDENTITY_KEY, JSON.stringify({ install_id: ids.b, secret: 'z', registered: false }));
			}
		};
		expect(store(storage as unknown as Storage).retire()).toBe(true);
		expect(JSON.parse(storage.data.get(IDENTITY_KEY)!).install_id).toBe(ids.b);
		expect(storage.data.has(RECEIPT_PREFIX + ids.a)).toBe(true);
	});

	it('keeps the identity when the receipt cannot be saved and still deletes with it', async () => {
		const storage = new FlakyStorage();
		const tab = makeTab({ storage: storage as unknown as Storage });
		setConsent('granted', 1);
		tab.telemetry.sync();
		const id = JSON.parse(storage.data.get(IDENTITY_KEY)!);
		storage.failSet = (key) => key.startsWith(RECEIPT_PREFIX);
		setConsent('denied', 2);
		tab.telemetry.sync();
		expect(storage.data.has(IDENTITY_KEY)).toBe(true);
		expect(tab.telemetry.snapshot()).toMatchObject({ active: false, error: 'receipt' });
		tab.calls.length = 0;
		expect(await tab.telemetry.deleteSent()).toEqual({ deleted: 1, failed: 0 });
		expect(tab.calls[0].url).toContain(id.install_id);
		expect(storage.data.has(IDENTITY_KEY)).toBe(false);
	});

	it('prunes expired receipts at startup', () => {
		const clock = makeClock();
		localStorage.setItem(RECEIPT_PREFIX + ids.a, JSON.stringify({ secret: 's', expiresAt: clock.now() - 1 }));
		localStorage.setItem(RECEIPT_PREFIX + ids.b, JSON.stringify({ secret: 's', expiresAt: clock.now() + 1000 }));
		makeTab({}, clock);
		expect(receiptKeys()).toEqual([RECEIPT_PREFIX + ids.b]);
	});
});

describe('queue resurrection is harmless', () => {
	it('a write that lands after the withdrawal is never sent and is cleared one second later', async () => {
		setConsent('granted', 1);
		const tab = makeTab();
		tab.telemetry.sync();
		tab.telemetry.track('app_opened');
		setConsent('denied', 2);
		tab.telemetry.sync();
		localStorage.setItem(QUEUE_KEY, JSON.stringify({ events: [entry()] }));
		expect(tab.later.map((l) => l.ms)).toContain(1000);
		setConsent('granted', 3);
		tab.later.forEach((l) => l.fn());
		await tick();
		expect(localStorage.getItem(QUEUE_KEY)).toBeNull();
	});

	it('events from another generation are purged on startup and never sent', async () => {
		localStorage.setItem(QUEUE_KEY, JSON.stringify({ events: [entry({ g: 1 }), entry({ id: ids.b, g: 3 })] }));
		setConsent('granted', 3);
		const tab = makeTab({}, makeClock());
		tab.telemetry.sync();
		expect(tab.telemetry.snapshot().pending).toBe(1);
		expect(stored(QUEUE_KEY).events.map((e: { g: number }) => e.g)).toEqual([3]);
		await tab.telemetry.flush();
		expect(bodyOf(tab.calls[0]).events.map((e: { id: string }) => e.id)).toEqual([ids.b]);
	});

	it('a flush sends only events of the current generation, checked right before the request', async () => {
		setConsent('granted', 1);
		const tab = makeTab({ sha256: async () => 'a'.repeat(64) });
		tab.telemetry.track('app_opened');
		const pending = tab.telemetry.flush();
		setConsent('granted', 2);
		await pending;
		expect(tab.calls).toEqual([]);
	});

	it('persists under the queue lock and drops the write when consent has gone by then', async () => {
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
});

describe('what leaves the device', () => {
	beforeEach(() => setConsent('granted', 1));

	it('reads every prop exactly once and sends what it validated', async () => {
		const tab = makeTab();
		let reads = 0;
		const props = {
			get route() {
				reads += 1;
				return reads === 1 ? 'Board' : 'Evil';
			},
		};
		tab.telemetry.track('screen_viewed', props as { route: 'Board' });
		await tab.telemetry.flush();
		expect(reads).toBe(1);
		expect(bodyOf(tab.calls[0]).events[0].p).toEqual({ route: 'Board' });
	});

	it('copies props at enqueue so later mutation cannot change the payload', async () => {
		const tab = makeTab();
		const props = { route: 'Board' as const };
		tab.telemetry.track('screen_viewed', props);
		(props as Record<string, unknown>).route = 'Evil';
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
		const clock = makeClock();
		const t = Math.floor(clock.now() / 3_600_000) * 3600;
		const u = (n: number) => `00000000-0000-4000-8000-0000000000c${n}`;
		localStorage.setItem(
			QUEUE_KEY,
			JSON.stringify({
				events: [
					{ ...entry({ id: u(0), t }), extra: 'x' },
					entry({ id: u(1), t: t + 11 }),
					entry({ id: u(2), t, n: 'nope' }),
					entry({ id: u(3), t, n: 'screen_viewed', p: { route: 'Secret' } }),
					entry({ id: u(4), t, p: { email: 'a@b.c' } }),
					entry({ id: u(5), t, m: { ...META, app_version: '3.4.87' } }),
					entry({ id: 'not-a-uuid', t }),
					entry({ id: u(6), t, seq: -1 }),
					entry({ id: u(7), t: 'x' }),
					entry({ id: u(8), t, g: 'x' }),
				],
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
