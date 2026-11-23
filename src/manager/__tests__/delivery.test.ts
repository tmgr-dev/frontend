import {
	CONFIRM_MS,
	canRetry,
	createDeliveryTracker,
	failureReason,
	isFailureNote,
	newMessageId,
	onAccepted,
	onConfirmTimeout,
	onReceipt,
	onRejected,
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
		expect(times).toEqual([CONFIRM_MS]);
		handles[0]();
		expect(t.get('a')?.state).toBe('unconfirmed');
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
});
