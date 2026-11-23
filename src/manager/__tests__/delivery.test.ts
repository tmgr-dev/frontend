import {
	canRetry,
	CONFIRM_MS,
	createDeliveryTracker,
	failureReason,
	isFailureNote,
	MAX_TRACKED,
	newMessageId,
	onAccepted,
	onConfirmTimeout,
	onReceipt,
	onRejected,
	onSendTimeout,
	SEND_TIMEOUT_REASON,
	SENDING_MS,
	startDelivery,
	type Delivery,
} from '../delivery';

describe('delivery transitions', () => {
	it('moves sending to sent when accepted, and leaves later states alone', () => {
		const d = startDelivery(true);
		expect(onAccepted(d).state).toBe('sent');
		const delivered = onReceipt(d, 'delivered');
		expect(onAccepted(delivered)).toBe(delivered);
	});

	it('fails with a reason on rejection', () => {
		expect(onRejected(startDelivery(true), 'nope')).toMatchObject({
			state: 'failed',
			reason: 'nope',
		});
	});

	it('receipts overwrite state and clear stale reasons', () => {
		const failed = onReceipt(startDelivery(true), 'failed', 'x');
		const ok = onReceipt(failed, 'delivered');
		expect(ok.state).toBe('delivered');
		expect(ok.reason).toBeUndefined();
	});

	it('a queued receipt keeps its reason and a later one without it drops it', () => {
		const queued = onReceipt(startDelivery(true), 'queued', 'turn');
		expect(queued).toMatchObject({ state: 'queued', reason: 'turn' });
		expect(onReceipt(queued, 'queued').reason).toBeUndefined();
		expect(onReceipt(queued, 'delivered').reason).toBeUndefined();
	});

	it('a message still sending fails with the timeout reason and can be retried', () => {
		const d = onSendTimeout(startDelivery(true));
		expect(d).toMatchObject({ state: 'failed', reason: SEND_TIMEOUT_REASON });
		expect(canRetry(d)).toBe(true);
		const sent = onAccepted(startDelivery(true));
		expect(onSendTimeout(sent)).toBe(sent);
	});

	it('times out only remote sent messages', () => {
		const sent: Delivery = { state: 'sent', remote: true };
		expect(onConfirmTimeout(sent).state).toBe('unconfirmed');
		expect(onConfirmTimeout({ ...sent, remote: false }).state).toBe('sent');
		expect(onConfirmTimeout({ ...sent, state: 'queued' }).state).toBe('queued');
	});

	it('allows retry for failed and unconfirmed rows that are not yet retried', () => {
		expect(canRetry({ state: 'failed', remote: true })).toBe(true);
		expect(canRetry({ state: 'unconfirmed', remote: true })).toBe(true);
		expect(canRetry({ state: 'sent', remote: true })).toBe(false);
		expect(canRetry({ state: 'failed', remote: true, retried: true })).toBe(
			false,
		);
		expect(canRetry(undefined)).toBe(false);
	});

	it('parses failure notes', () => {
		expect(isFailureNote('Not delivered: busy')).toBe(true);
		expect(isFailureNote('Delivered')).toBe(false);
		expect(failureReason('Not delivered: busy now')).toBe('busy now');
		expect(failureReason('Not delivered')).toBe('');
	});

	it('generates valid message ids', () => {
		const id = newMessageId();
		expect(id).toMatch(/^m_[0-9a-f]{32}$/);
		expect(newMessageId()).not.toBe(id);
	});
});

describe('delivery tracker', () => {
	beforeEach(() => jest.useFakeTimers());
	afterEach(() => jest.useRealTimers());

	it('confirms after the timeout using the injected timers', () => {
		const handles: Array<() => void> = [];
		const times: number[] = [];
		const seen: Array<Delivery | undefined> = [];
		const t = createDeliveryTracker((_, d) => seen.push(d), {
			set: (fn, ms) => {
				handles.push(fn);
				times.push(ms);
				return handles.length;
			},
			clear: () => undefined,
		});
		t.begin('a', true);
		t.accepted('a');
		expect(times).toEqual([SENDING_MS, CONFIRM_MS]);
		handles[1]();
		expect(t.get('a')?.state).toBe('unconfirmed');
	});

	it('fails a message that is still sending after the sending timeout, for local rows too', () => {
		const t = createDeliveryTracker(() => undefined);
		t.begin('a', true);
		t.begin('b', false);
		jest.advanceTimersByTime(SENDING_MS - 1);
		expect(t.get('a')?.state).toBe('sending');
		jest.advanceTimersByTime(1);
		expect(t.get('a')).toMatchObject({
			state: 'failed',
			reason: SEND_TIMEOUT_REASON,
		});
		expect(t.get('b')).toMatchObject({
			state: 'failed',
			reason: SEND_TIMEOUT_REASON,
		});
	});

	it('an accepted or rejected message never gets the sending timeout', () => {
		const t = createDeliveryTracker(() => undefined);
		t.begin('a', true);
		t.accepted('a');
		t.begin('b', true);
		t.rejected('b', 'down');
		jest.advanceTimersByTime(SENDING_MS + 1);
		expect(t.get('a')?.state).toBe('unconfirmed');
		expect(t.get('b')).toMatchObject({ state: 'failed', reason: 'down' });
	});

	it('uses fake timers by default and cancels on receipt', () => {
		const t = createDeliveryTracker(() => undefined);
		t.begin('a', true);
		t.accepted('a');
		t.receipt('a', 'queued');
		jest.advanceTimersByTime(CONFIRM_MS * 2);
		expect(t.get('a')?.state).toBe('queued');
	});

	it('a receipt after unconfirmed wins', () => {
		const t = createDeliveryTracker(() => undefined);
		t.begin('a', true);
		t.accepted('a');
		jest.advanceTimersByTime(CONFIRM_MS);
		expect(t.get('a')?.state).toBe('unconfirmed');
		t.receipt('a', 'delivered');
		expect(t.get('a')?.state).toBe('delivered');
	});

	it('forgets local deliveries on accept and moves entries on rename', () => {
		const t = createDeliveryTracker(() => undefined);
		t.begin('l', false);
		t.accepted('l');
		expect(t.get('l')).toBeUndefined();
		t.begin('x', true);
		t.rename('x', 'y');
		expect(t.get('x')).toBeUndefined();
		expect(t.get('y')?.state).toBe('sending');
	});

	it('marks retried', () => {
		const t = createDeliveryTracker(() => undefined);
		t.begin('a', true);
		t.rejected('a', 'e');
		t.retried('a');
		expect(t.get('a')?.retried).toBe(true);
	});

	it('drops the oldest entry past the cap and clears its confirm timer', () => {
		const cleared: unknown[] = [];
		const seen: Array<[string, Delivery | undefined]> = [];
		const t = createDeliveryTracker((id, d) => seen.push([id, d]), {
			set: (_, __) => 'h',
			clear: (h) => cleared.push(h),
		});
		t.begin('first', true);
		t.accepted('first');
		for (let i = 0; i < MAX_TRACKED; i++) t.begin(`m${i}`, true);
		expect(t.get('first')).toBeUndefined();
		expect(t.get('m0')?.state).toBe('sending');
		expect(cleared).toEqual(['h', 'h']);
		expect(seen.filter(([id, d]) => id === 'first' && !d)).toHaveLength(1);
	});

	it('evicts finished entries before one still in flight', () => {
		const t = createDeliveryTracker(() => {}, {
			set: () => 'h',
			clear: () => {},
		});
		t.begin('pending', true);
		t.begin('done', true);
		t.accepted('done');
		t.receipt('done', 'delivered');
		for (let i = 0; i < MAX_TRACKED - 2; i++) t.begin(`m${i}`, true);
		t.begin('overflow', true);
		expect(t.get('done')).toBeUndefined();
		expect(t.get('pending')?.state).toBe('sending');
		expect(t.get('overflow')).toBeDefined();
	});

	it('treats failed and retried entries as finished when evicting', () => {
		const t = createDeliveryTracker(() => {}, {
			set: () => 'h',
			clear: () => {},
		});
		t.begin('pending', true);
		t.begin('bad', true);
		t.rejected('bad', 'nope');
		for (let i = 0; i < MAX_TRACKED - 2; i++) t.begin(`m${i}`, true);
		t.begin('overflow', true);
		expect(t.get('bad')).toBeUndefined();
		expect(t.get('pending')).toBeDefined();
	});
});
