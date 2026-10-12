/** @jest-environment jsdom */
import { createHash } from 'node:crypto';
import { QUEUE_KEY } from '../client';
import { IDENTITY_KEY } from '../identity';
import { BASE, FakeChannel, makeClock, makeTab, receiptKeys, setConsent, tick } from './harness';

beforeEach(() => {
	localStorage.clear();
	FakeChannel.all = [];
});

afterEach(tick);

const stored = (key: string) => JSON.parse(localStorage.getItem(key) ?? 'null');
const bodyOf = (call: { init: RequestInit }) => JSON.parse(call.init.body as string);

describe('without consent', () => {
	it.each([null, 'denied'] as const)('sends nothing and stores no id when usage is %s', async (usage) => {
		if (usage) setConsent(usage, 1);
		const tab = makeTab();
		tab.telemetry.sync();
		tab.telemetry.track('app_opened');
		await tab.telemetry.flush();
		await tick();
		expect(tab.calls).toEqual([]);
		expect(localStorage.getItem(IDENTITY_KEY)).toBeNull();
		expect(localStorage.getItem(QUEUE_KEY)).toBeNull();
		expect(tab.timers).toEqual([]);
		expect(tab.telemetry.snapshot()).toEqual({ active: false, installId: null, pending: 0, error: null });
	});
});

describe('with consent', () => {
	beforeEach(() => setConsent('granted', 1));

	it('creates the identity at opt-in and sends secret_sha256 until the first 204', async () => {
		const tab = makeTab();
		tab.telemetry.sync();
		const identity = stored(IDENTITY_KEY);
		expect(identity.secret).toMatch(/^[0-9a-f]{64}$/);
		tab.telemetry.track('app_opened');
		await tab.telemetry.flush();
		tab.telemetry.track('feature_used', { feature: 'timer_started' });
		await tab.telemetry.flush();
		expect(tab.calls).toHaveLength(2);
		expect(bodyOf(tab.calls[0]).secret_sha256).toBe(
			createHash('sha256').update(identity.secret).digest('hex'),
		);
		expect(bodyOf(tab.calls[1])).not.toHaveProperty('secret_sha256');
		expect(stored(IDENTITY_KEY).registered).toBe(true);
	});

	it('keeps sending the secret hash while the server does not answer 204', async () => {
		const tab = makeTab();
		tab.respond.status = 500;
		tab.telemetry.track('app_opened');
		await tab.telemetry.flush();
		tab.clock.advance(31 * 60_000);
		tab.respond.status = 204;
		await tab.telemetry.flush();
		expect(bodyOf(tab.calls[1]).secret_sha256).toBeDefined();
	});

	it('uses a plain credential-less request on every call', async () => {
		const tab = makeTab();
		tab.telemetry.track('app_opened');
		await tab.telemetry.flush({ keepalive: true });
		await tab.telemetry.deleteSent();
		expect(tab.calls.length).toBeGreaterThanOrEqual(2);
		tab.calls.forEach(({ init }) => {
			expect(init.credentials).toBe('omit');
			const names = Object.keys(init.headers ?? {}).map((h) => h.toLowerCase());
			expect(names).not.toContain('authorization');
			expect(names).not.toContain('cookie');
		});
		expect(tab.calls[0].url).toBe(`${BASE}t/events`);
		expect(tab.calls[0].init.keepalive).toBe(true);
	});

	it('builds an envelope from enums only, with t truncated to the hour', async () => {
		const tab = makeTab({ secret: () => 'a'.repeat(64), uuid: (() => { let i = 0; return () => `00000000-0000-4000-8000-${String(i++).padStart(12, '0')}`; })() });
		tab.telemetry.track('screen_viewed', { route: 'WorkspaceBoard' });
		await tab.telemetry.flush();
		expect(bodyOf(tab.calls[0])).toMatchSnapshot({
			secret_sha256: expect.stringMatching(/^[0-9a-f]{64}$/),
		});
		expect(bodyOf(tab.calls[0]).events[0].t % 3600).toBe(0);
	});

	it('ignores events and props that are not in the catalog', async () => {
		const tab = makeTab();
		const track = tab.telemetry.track as (n: string, p?: unknown) => void;
		track('not_an_event');
		track('screen_viewed', { route: 'SecretRoute' });
		track('screen_viewed', { route: 'Board', extra: 'x' });
		track('tour_step_viewed', { tour: 'time.timer', step: 'nav.sidebar', i: 1 });
		track('tour_step_viewed', { tour: 'time.timer', step: 'task-form.timer', i: 99 });
		expect(tab.telemetry.snapshot().pending).toBe(0);
		track('tour_step_viewed', { tour: 'time.timer', step: 'task-form.timer', i: 1 });
		expect(tab.telemetry.snapshot().pending).toBe(1);
	});

	it('caps the queue at 200 and drops the oldest', () => {
		const tab = makeTab();
		for (let i = 0; i < 205; i++) tab.telemetry.track('connect_command_copied');
		expect(tab.telemetry.snapshot().pending).toBe(200);
		const seqs = JSON.parse(tab.telemetry.queueJson()).map((e: { seq: number }) => e.seq);
		expect(seqs[0]).toBe(5);
	});

	it('sends at most 50 events per batch', async () => {
		const tab = makeTab();
		for (let i = 0; i < 120; i++) tab.telemetry.track('connect_command_copied');
		await tab.telemetry.flush();
		expect(tab.calls.map((c) => bodyOf(c).events.length)).toEqual([50, 50, 20]);
	});

	it('drops events older than 7 days at flush', async () => {
		const tab = makeTab();
		tab.telemetry.track('connect_command_copied');
		tab.clock.advance(8 * 24 * 3600_000);
		tab.telemetry.track('connect_stalled');
		await tab.telemetry.flush();
		expect(bodyOf(tab.calls[0]).events.map((e: { n: string }) => e.n)).toEqual(['connect_stalled']);
	});

	it('mirrors the queue to storage and reloads it, dropping stale entries', async () => {
		const tab = makeTab();
		tab.telemetry.track('connect_command_copied');
		await tick();
		expect(stored(QUEUE_KEY).events).toHaveLength(1);
		const reloaded = makeTab();
		reloaded.telemetry.sync();
		expect(reloaded.telemetry.snapshot().pending).toBe(1);
		const old = makeTab({}, makeClock(Date.UTC(2026, 9, 25)));
		old.telemetry.sync();
		expect(old.telemetry.snapshot().pending).toBe(0);
	});

	it('flushes on the 30 second timer and backs off after a failure', async () => {
		const tab = makeTab();
		tab.telemetry.track('connect_command_copied');
		expect(tab.timers[0].ms).toBe(30_000);
		tab.respond.status = 429;
		tab.timers[0].fn();
		await tick();
		expect(tab.calls).toHaveLength(1);
		tab.timers[0].fn();
		await tick();
		expect(tab.calls).toHaveLength(1);
		tab.clock.advance(31_000);
		tab.respond.status = 204;
		tab.timers[0].fn();
		await tick();
		expect(tab.calls).toHaveLength(2);
		expect(tab.telemetry.snapshot().pending).toBe(0);
	});

	it('does not retry a rejected batch forever', async () => {
		const tab = makeTab();
		tab.telemetry.track('connect_command_copied');
		tab.respond.status = 400;
		await tab.telemetry.flush();
		expect(tab.telemetry.snapshot().pending).toBe(0);
	});
});

describe('withdrawal', () => {
	beforeEach(() => setConsent('granted', 1));

	it('aborts a flush that is waiting on the network and sends nothing afterwards', async () => {
		let release!: () => void;
		const tab = makeTab();
		tab.respond.delay = new Promise((resolve) => (release = resolve));
		tab.telemetry.track('app_opened');
		const id = stored(IDENTITY_KEY);
		const pending = tab.telemetry.flush();
		await tick();
		expect(tab.calls).toHaveLength(1);
		const signal = tab.calls[0].init.signal as AbortSignal;
		setConsent('denied', 2);
		await tab.telemetry.withdraw();
		expect(signal.aborted).toBe(true);
		release();
		await pending;
		tab.telemetry.track('app_opened');
		await tab.telemetry.flush();
		await tick();
		expect(tab.calls).toHaveLength(1);
		expect(tab.telemetry.snapshot()).toEqual({ active: false, installId: null, pending: 0, error: null });
		expect(localStorage.getItem(IDENTITY_KEY)).toBeNull();
		expect(localStorage.getItem(QUEUE_KEY)).toBeNull();
		expect(receiptKeys()).toEqual([`tmgr:telemetry:receipt:${id.install_id}`]);
		expect(stored(receiptKeys()[0]).secret).toBe(id.secret);
	});

	it('does not send when consent flips while the secret hash is being computed', async () => {
		let finish!: (hash: string) => void;
		const tab = makeTab({ sha256: () => new Promise((resolve) => (finish = resolve)) });
		tab.telemetry.track('app_opened');
		const pending = tab.telemetry.flush();
		await tick();
		setConsent('denied', 2);
		finish('abc');
		await pending;
		expect(tab.calls).toEqual([]);
	});

	it('does not send when only the generation changed (off then on elsewhere)', async () => {
		const tab = makeTab();
		tab.telemetry.track('app_opened');
		setConsent('granted', 3);
		await tab.telemetry.flush();
		expect(tab.calls).toEqual([]);
		expect(tab.telemetry.snapshot().pending).toBe(0);
	});

	it('a queue write that finishes after the clear does not bring events back', async () => {
		const tab = makeTab();
		tab.telemetry.track('app_opened');
		tab.telemetry.track('connect_stalled');
		setConsent('denied', 2);
		tab.telemetry.sync();
		await tick();
		await tick();
		expect(localStorage.getItem(QUEUE_KEY)).toBeNull();
	});

	it('stops a second tab on a storage change and clears its queue', async () => {
		const a = makeTab();
		const b = makeTab();
		a.telemetry.sync();
		b.telemetry.sync();
		b.telemetry.track('app_opened');
		expect(b.telemetry.snapshot().active).toBe(true);
		setConsent('denied', 2);
		b.telemetry.sync();
		expect(b.telemetry.snapshot()).toMatchObject({ active: false, pending: 0, error: null });
		expect(localStorage.getItem(IDENTITY_KEY)).not.toBeNull();
		expect(b.timers[0].cleared).toBe(true);
		a.telemetry.sync();
		expect(a.telemetry.snapshot().active).toBe(false);
	});
});

describe('receipts and deletion', () => {
	beforeEach(() => setConsent('granted', 1));

	const optOutWithReceipt = async (tab = makeTab()) => {
		tab.telemetry.track('app_opened');
		await tab.telemetry.flush();
		const id = stored(IDENTITY_KEY);
		setConsent('denied', 2);
		await tab.telemetry.withdraw();
		return { tab, id };
	};

	it('deletes after opt-out with the secret header, and 204 removes the receipt', async () => {
		const { tab, id } = await optOutWithReceipt();
		tab.calls.length = 0;
		const result = await tab.telemetry.deleteSent();
		expect(result).toEqual({ deleted: 1, failed: 0 });
		expect(tab.calls[0].url).toBe(`${BASE}t/installs/${id.install_id}`);
		expect(tab.calls[0].init.method).toBe('DELETE');
		expect(tab.calls[0].init.headers['X-Telemetry-Secret']).toBe(id.secret);
		expect(receiptKeys()).toEqual([]);
	});

	it('treats 404 as done and keeps the receipt on 500', async () => {
		const { tab } = await optOutWithReceipt();
		tab.respond.status = 500;
		expect(await tab.telemetry.deleteSent()).toEqual({ deleted: 0, failed: 1 });
		expect(receiptKeys()).toHaveLength(1);
		tab.respond.status = 'network';
		expect(await tab.telemetry.deleteSent()).toEqual({ deleted: 0, failed: 1 });
		tab.respond.status = 404;
		expect(await tab.telemetry.deleteSent()).toEqual({ deleted: 1, failed: 0 });
		expect(receiptKeys()).toEqual([]);
	});

	it('stops collection before deleting while sharing is still on', async () => {
		const tab = makeTab();
		tab.telemetry.track('app_opened');
		await tab.telemetry.flush();
		const result = await tab.telemetry.deleteSent();
		expect(result.deleted).toBe(1);
		expect(tab.telemetry.snapshot().active).toBe(false);
		expect(tab.calls[tab.calls.length - 1].init.method).toBe('DELETE');
	});

	it('forgets receipts after 30 days', async () => {
		const { tab } = await optOutWithReceipt();
		tab.clock.advance(31 * 24 * 3600_000);
		tab.calls.length = 0;
		expect(await tab.telemetry.deleteSent()).toEqual({ deleted: 0, failed: 0 });
		expect(tab.calls).toEqual([]);
	});

	it('Reset ID retires the old id as a receipt and creates a new one', async () => {
		const tab = makeTab();
		tab.telemetry.sync();
		const before = stored(IDENTITY_KEY);
		await tab.telemetry.resetId();
		const after = stored(IDENTITY_KEY);
		expect(after.install_id).not.toBe(before.install_id);
		expect(after.registered).toBe(false);
		expect(receiptKeys()).toEqual([`tmgr:telemetry:receipt:${before.install_id}`]);
	});

	it('Reset ID does nothing while sharing is off', () => {
		setConsent('denied', 2);
		const tab = makeTab();
		tab.telemetry.resetId();
		expect(localStorage.getItem(IDENTITY_KEY)).toBeNull();
		expect(receiptKeys()).toEqual([]);
	});
});
