/** @jest-environment jsdom */
import { createConsentStore } from '@/privacy/consent';
import { localStorageOf } from '@/privacy/storage';
import { attachTelemetry } from '../attach';
import { createUsageConsent, USAGE_KEY } from '../consent';
import { createIdentityStore, IDENTITY_KEY, RECEIPT_PREFIX } from '../identity';
import {
	deferred,
	FakeChannel,
	FlakyStorage,
	makeClock,
	makeTab,
	setConsent,
	TestLocks,
	testWindow,
	tick,
} from './harness';

const ID_A = '00000000-0000-4000-8000-00000000000a';
const ID_B = '00000000-0000-4000-8000-00000000000b';
const SECRET = 'a'.repeat(64);
const identity = (install_id = ID_A) => ({
	install_id,
	secret: SECRET,
	registered: false,
});
const asStorage = (storage: FlakyStorage) => storage as unknown as Storage;
const body = (call: { init: RequestInit }) =>
	JSON.parse(call.init.body as string);

beforeEach(() => {
	localStorage.clear();
	FakeChannel.all = [];
});
afterEach(tick);

const connectedTab = (storage: FlakyStorage, locks?: TestLocks) => {
	const win = testWindow(storage, locks);
	const usage = createUsageConsent(win);
	const tab = makeTab({
		storage: asStorage(storage),
		locks,
		consent: { read: usage.read, deny: () => usage.set(false) },
	});
	attachTelemetry(tab.telemetry, usage, win);
	return { ...tab, usage, win };
};

it('serializes overlapping grant and denial generation reads and writes', async () => {
	const storage = new FlakyStorage();
	const locks = new TestLocks();
	storage.setItem(USAGE_KEY, JSON.stringify({ on: true, generation: 1 }));
	const aWin = testWindow(storage, locks);
	const bWin = testWindow(storage, locks);
	Object.assign(aWin, { BroadcastChannel: undefined });
	Object.assign(bWin, { BroadcastChannel: undefined });
	const a = createUsageConsent(aWin);
	const b = createUsageConsent(bWin);
	await tick();
	const read = storage.getItem.bind(storage);
	let deny: unknown;
	let armed = true;
	storage.getItem = (key) => {
		const value = read(key);
		if (key === USAGE_KEY && armed) {
			armed = false;
			deny = b.set(false);
		}
		return value;
	};
	await a.set(true);
	await deny;
	expect(JSON.parse(read(USAGE_KEY)!).on).toBe(false);
	expect(locks.requests).toContain('tmgr-telemetry-consent');
});

it('cannot persist withdrawal between the final consent read and fetch start', async () => {
	const storage = new FlakyStorage();
	const locks = new TestLocks();
	storage.setItem(USAGE_KEY, JSON.stringify({ on: true, generation: 1 }));
	storage.setItem(
		IDENTITY_KEY,
		JSON.stringify({ ...identity(), registered: true }),
	);
	const sender = createUsageConsent(testWindow(storage, locks));
	const withdrawer = createUsageConsent(testWindow(storage, locks));
	await tick();
	let reads = 0;
	let armed = false;
	let denial: unknown;
	const posts: { on: boolean; signal: AbortSignal }[] = [];
	const tab = makeTab({
		storage: asStorage(storage),
		locks,
		consent: {
			read: () => {
				const view = sender.read();
				if (armed && (++reads === 2 || locks.held.has('tmgr-telemetry-send'))) {
					armed = false;
					denial = withdrawer.set(false);
				}
				return view;
			},
			deny: () => sender.set(false),
		},
		fetch: (async (_url: string, init: RequestInit) => {
			posts.push({
				on: JSON.parse(storage.getItem(USAGE_KEY)!).on,
				signal: init.signal!,
			});
			return { status: 204 } as Response;
		}) as unknown as typeof fetch,
	});
	tab.telemetry.track('app_opened');
	await tick();
	armed = true;
	await tab.telemetry.flush();
	await denial;
	const before = posts.length;
	tab.telemetry.track('app_opened');
	await tab.telemetry.flush();
	expect(posts.length).toBe(before);
	expect(JSON.parse(storage.getItem(USAGE_KEY)!).on).toBe(false);
});

it('holds the send lock at fetch start and releases it before the response', async () => {
	const storage = new FlakyStorage();
	const locks = new TestLocks();
	const tab = connectedTab(storage, locks);
	await tab.usage.set(true);
	tab.telemetry.track('app_opened');
	const response = deferred();
	tab.respond.delay = response.promise;
	let heldAtStart: string[] = [];
	tab.respond.onStart = () => {
		heldAtStart = [...locks.held];
	};
	const pending = tab.telemetry.flush();
	await tick();
	expect(tab.calls).toHaveLength(1);
	expect(locks.requests).toContain('tmgr-telemetry-send');
	expect(heldAtStart).toEqual(
		expect.arrayContaining(['tmgr-telemetry-consent', 'tmgr-telemetry-send']),
	);
	expect(locks.held.has('tmgr-telemetry-send')).toBe(false);
	await tab.usage.set(false);
	expect(tab.calls[0].init.signal?.aborted).toBe(true);
	expect(storage.getItem(USAGE_KEY)).toContain('false');
	response.resolve();
	await pending;
});

it('adopts every delivered deny despite unreadable storage and a higher stored generation', async () => {
	const storage = new FlakyStorage();
	storage.setItem(USAGE_KEY, JSON.stringify({ on: true, generation: 20 }));
	const b = connectedTab(storage);
	b.telemetry.track('app_opened');
	const response = deferred();
	b.respond.delay = response.promise;
	const pending = b.telemetry.flush();
	await tick();
	const win = testWindow(storage);
	Object.defineProperty(win, 'localStorage', {
		value: {
			getItem: () => {
				throw new Error('blocked');
			},
			setItem: () => {
				throw new Error('blocked');
			},
			removeItem: () => {
				throw new Error('blocked');
			},
		},
	});
	let a: ReturnType<typeof createUsageConsent>;
	expect(() => {
		a = createUsageConsent(win);
	}).not.toThrow();
	await a!.set(false);
	expect(b.usage.read().on).toBe(false);
	expect(b.telemetry.snapshot()).toMatchObject({ active: false, pending: 0 });
	expect(b.timers.every((timer) => timer.cleared)).toBe(true);
	expect(b.calls[0].init.signal?.aborted).toBe(true);
	expect(storage.getItem(IDENTITY_KEY)).not.toBeNull();
	expect(
		[...storage.data.keys()].some((key) => key.startsWith(RECEIPT_PREFIX)),
	).toBe(false);
	response.resolve();
	await pending;
	b.telemetry.track('app_opened');
	await b.telemetry.flush();
	expect(b.calls).toHaveLength(1);
	storage.setItem(USAGE_KEY, JSON.stringify({ on: true, generation: 21 }));
	b.win.fire('storage', {
		key: USAGE_KEY,
		newValue: storage.getItem(USAGE_KEY),
	});
	expect(b.usage.read().on).toBe(true);
});

it.each([undefined, -1, 0, 1])(
	'always adopts a delivered deny with generation %s',
	async (generation) => {
		const storage = new FlakyStorage();
		storage.setItem(USAGE_KEY, JSON.stringify({ on: true, generation: 20 }));
		const tab = connectedTab(storage);
		tab.telemetry.track('app_opened');
		new FakeChannel().postMessage({ type: 'deny', generation });
		await tab.telemetry.flush();
		expect(tab.calls).toHaveLength(0);
		expect(tab.telemetry.snapshot()).toMatchObject({
			active: false,
			pending: 0,
		});
	},
);

it('serializes retirement with a reset that starts during compare-and-delete', async () => {
	const storage = new FlakyStorage();
	const locks = new TestLocks();
	storage.setItem(IDENTITY_KEY, JSON.stringify(identity()));
	const deps = {
		storage: asStorage(storage),
		locks,
		now: makeClock().now,
		uuid: () => ID_B,
		secret: () => SECRET,
	};
	const a = createIdentityStore(deps);
	const b = createIdentityStore(deps);
	const read = storage.getItem.bind(storage);
	let reads = 0;
	let reset: unknown;
	storage.getItem = (key) => {
		const value = read(key);
		if (key === IDENTITY_KEY && ++reads === 2) reset = b.rotate();
		return value;
	};
	await a.retire();
	await reset;
	expect(b.read()?.install_id).toBe(ID_B);
	expect(storage.getItem(RECEIPT_PREFIX + ID_A)).not.toBeNull();
	expect(
		locks.requests.filter((name) => name === 'tmgr-telemetry-identity').length,
	).toBeGreaterThanOrEqual(2);
});

it('does not prune a receipt refreshed after its expired value was read', async () => {
	const storage = new FlakyStorage();
	const locks = new TestLocks();
	const clock = makeClock();
	const key = RECEIPT_PREFIX + ID_A;
	storage.setItem(
		key,
		JSON.stringify({ secret: SECRET, expiresAt: clock.now() - 1 }),
	);
	const fresh = JSON.stringify({
		secret: SECRET,
		expiresAt: clock.now() + 60_000,
	});
	const read = storage.getItem.bind(storage);
	let refreshed = false;
	storage.getItem = (name) => {
		const value = read(name);
		if (name === key && !refreshed) {
			refreshed = true;
			storage.setItem(key, fresh);
		}
		return value;
	};
	const deps = {
		storage: asStorage(storage),
		locks,
		now: clock.now,
		uuid: () => ID_B,
		secret: () => SECRET,
	};
	await createIdentityStore(deps).receipts();
	expect(read(key)).toBe(fresh);
	expect(locks.requests).toContain('tmgr-telemetry-identity');
});

it('identity storage events never retire or recreate another tab identity in a hardOff tab', async () => {
	const storage = new FlakyStorage();
	const a = connectedTab(storage);
	await a.usage.set(true);
	await a.usage.set(false);
	const b = connectedTab(storage);
	await b.usage.set(true);
	const current = storage.getItem(IDENTITY_KEY);
	const receipts = [...storage.data.keys()].filter((key) =>
		key.startsWith(RECEIPT_PREFIX),
	);
	for (let i = 0; i < 5; i += 1) {
		a.win.fire('storage', { key: IDENTITY_KEY });
		b.win.fire('storage', { key: IDENTITY_KEY });
		await tick();
	}
	expect(storage.getItem(IDENTITY_KEY)).toBe(current);
	expect(
		[...storage.data.keys()].filter((key) => key.startsWith(RECEIPT_PREFIX)),
	).toEqual(receipts);
	expect(a.usage.read().on).toBe(false);
	expect(b.telemetry.snapshot().active).toBe(true);
});

it.each([
	{ install_id: 'someone@example.test', secret: SECRET },
	{ install_id: '00000000-0000-1000-8000-00000000000a', secret: SECRET },
	{ install_id: ID_A, secret: 'bad secret' },
])('never sends a malformed stored identity: %j', async (poison) => {
	setConsent('granted', 1);
	localStorage.setItem(
		IDENTITY_KEY,
		JSON.stringify({ ...poison, registered: false }),
	);
	const store = createIdentityStore({
		storage: localStorage,
		now: Date.now,
		uuid: () => ID_B,
		secret: () => SECRET,
	});
	expect(store.read()).toBeNull();
	const tab = makeTab();
	tab.telemetry.track('app_opened');
	await tab.telemetry.flush();
	expect(tab.calls).toHaveLength(1);
	expect(body(tab.calls[0]).install_id).not.toBe(poison.install_id);
});

it('rejects non-string event names without coercion', async () => {
	setConsent('granted', 1);
	const tab = makeTab();
	const toString = jest.fn(() => 'app_opened');
	const track = tab.telemetry.track as (name: unknown) => void;
	track({ toString });
	track(['app_opened']);
	track(new String('app_opened'));
	await tab.telemetry.flush();
	expect(toString).not.toHaveBeenCalled();
	expect(tab.calls).toHaveLength(0);
});

it('rejects unexpected keys before reading any prop values', () => {
	setConsent('granted', 1);
	const tab = makeTab();
	const read = jest.fn(() => {
		throw new Error('getter');
	});
	const props = {
		get route() {
			return read();
		},
		get extra() {
			return read();
		},
	};
	expect(() => tab.telemetry.track('screen_viewed', props)).not.toThrow();
	expect(read).not.toHaveBeenCalled();
	expect(tab.telemetry.snapshot().pending).toBe(0);
});

it('drops props whose allowed getter throws', () => {
	setConsent('granted', 1);
	const tab = makeTab();
	expect(() =>
		tab.telemetry.track('screen_viewed', {
			get route(): 'Board' {
				throw new Error('getter');
			},
		}),
	).not.toThrow();
	expect(tab.telemetry.snapshot().pending).toBe(0);
});

it('an old install acknowledgement cannot register the new install', async () => {
	setConsent('granted', 1);
	const a = makeTab();
	a.telemetry.track('app_opened');
	const response = deferred();
	a.respond.delay = response.promise;
	const pending = a.telemetry.flush();
	await tick();
	const b = makeTab();
	b.telemetry.sync();
	await b.telemetry.resetId();
	const next = JSON.parse(localStorage.getItem(IDENTITY_KEY)!);
	response.resolve();
	await pending;
	expect(JSON.parse(localStorage.getItem(IDENTITY_KEY)!).registered).toBe(
		false,
	);
	b.telemetry.track('app_opened');
	await b.telemetry.flush();
	expect(body(b.calls[0])).toMatchObject({
		install_id: next.install_id,
		secret_sha256: expect.stringMatching(/^[0-9a-f]{64}$/),
	});
});

it('consent stores survive a throwing localStorage getter on creation, reads and writes', async () => {
	const win = testWindow(null);
	Object.defineProperty(win, 'localStorage', {
		get: () => {
			throw new Error('blocked');
		},
	});
	const analytics = createConsentStore(win);
	expect(() => analytics.set({ analytics: 'denied' })).not.toThrow();
	const usage = createUsageConsent(win);
	expect(usage.read().on).toBe(false);
	await usage.set(true);
	expect(usage.read().on).toBe(false);
	expect(usage.saveFailed()).toBe(true);
	await usage.set(false);
	expect(usage.read()).toMatchObject({ on: false, answered: true });
});

it('stops a flush waiting for the send lock when this window withdraws', async () => {
	const storage = new FlakyStorage();
	const locks = new TestLocks();
	const tab = connectedTab(storage, locks);
	await tab.usage.set(true);
	tab.telemetry.track('app_opened');
	const gate = deferred();
	const held = locks.request('tmgr-telemetry-send', () => gate.promise);
	const flush = tab.telemetry.flush();
	await tick();
	const denial = tab.usage.set(false);
	expect(tab.telemetry.snapshot()).toMatchObject({ active: false, pending: 0 });
	gate.resolve();
	await Promise.all([held, denial, flush]);
	expect(tab.calls).toHaveLength(0);
	expect(storage.getItem(IDENTITY_KEY)).toBeNull();
});

it('concurrent identity creation reuses the identity already created under the lock', async () => {
	const storage = new FlakyStorage();
	const locks = new TestLocks();
	const deps = {
		storage: asStorage(storage),
		locks,
		now: Date.now,
		secret: () => SECRET,
	};
	const a = createIdentityStore({ ...deps, uuid: () => ID_A });
	const b = createIdentityStore({ ...deps, uuid: () => ID_B });
	const [first, second] = await Promise.all([a.create(), b.create()]);
	expect(second).toEqual(first);
	expect(a.read()).toEqual(first);
});

it('does not send an identity reset by another tab during hashing', async () => {
	setConsent('granted', 1);
	const hash = deferred<string>();
	const locks = new TestLocks();
	const a = makeTab({ locks, sha256: () => hash.promise });
	a.telemetry.track('app_opened');
	const sending = a.telemetry.flush();
	await tick();
	const oldId = a.telemetry.snapshot().installId;
	const b = makeTab({ locks });
	b.telemetry.sync();
	await b.telemetry.resetId();
	hash.resolve('a'.repeat(64));
	await sending;
	expect(a.calls).toHaveLength(0);
	expect(localStorage.getItem(RECEIPT_PREFIX + oldId)).not.toBeNull();
	await a.telemetry.flush();
	expect(body(a.calls[0]).install_id).toBe(b.telemetry.snapshot().installId);
});

it('starts safely when obtaining localStorage throws', async () => {
	const win = testWindow(null);
	Object.defineProperty(win, 'localStorage', {
		get: () => {
			throw new Error('blocked');
		},
	});
	setConsent('granted', 1);
	const tab = makeTab({ storage: localStorageOf(win) });
	tab.telemetry.track('app_opened');
	await tab.telemetry.flush();
	expect(tab.calls).toHaveLength(0);
	expect(tab.telemetry.snapshot()).toMatchObject({
		active: false,
		installId: null,
	});
});

it('leaves the never-shipped legacy receipt key alone', async () => {
	const legacy = JSON.stringify([{ ...identity(), created: Date.now() }]);
	localStorage.setItem('tmgr:telemetry:receipts', legacy);
	const store = createIdentityStore({
		storage: localStorage,
		now: Date.now,
		uuid: () => ID_B,
		secret: () => SECRET,
	});
	expect(await store.receipts()).toEqual([]);
	expect(localStorage.getItem('tmgr:telemetry:receipts')).toBe(legacy);
});

it('a delayed old storage grant cannot undo a delivered denial', async () => {
	const storage = new FlakyStorage();
	const original = JSON.stringify({ on: true, generation: 7 });
	storage.setItem(USAGE_KEY, original);
	const tab = connectedTab(storage);
	tab.telemetry.track('app_opened');
	new FakeChannel().postMessage({ type: 'deny', generation: 1 });
	tab.win.fire('storage', {
		key: USAGE_KEY,
		oldValue: null,
		newValue: original,
	});
	tab.win.fire('storage', {
		key: USAGE_KEY,
		oldValue: original,
		newValue: null,
	});
	tab.win.fire('storage', { key: null, oldValue: null, newValue: null });
	tab.telemetry.track('app_opened');
	await tab.telemetry.flush();
	expect(tab.calls).toHaveLength(0);
	expect(tab.usage.read().on).toBe(false);
	const granted = JSON.stringify({ on: true, generation: 8 });
	storage.setItem(USAGE_KEY, granted);
	tab.win.fire('storage', {
		key: USAGE_KEY,
		oldValue: original,
		newValue: granted,
	});
	tab.telemetry.track('app_opened');
	await tab.telemetry.flush();
	expect(tab.calls).toHaveLength(1);
});

it('persists a withdrawal synchronously, before any lock is granted', async () => {
	const storage = new FlakyStorage();
	const locks = new TestLocks();
	storage.setItem(USAGE_KEY, JSON.stringify({ on: true, generation: 1 }));
	const usage = createUsageConsent(testWindow(storage, locks));
	await tick();
	const pending = usage.set(false);
	expect(JSON.parse(storage.getItem(USAGE_KEY)!).on).toBe(false);
	await pending;
	expect(JSON.parse(storage.getItem(USAGE_KEY)!).on).toBe(false);
});
