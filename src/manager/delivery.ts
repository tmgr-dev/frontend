export type DeliveryState =
	'sending' | 'sent' | 'queued' | 'delivered' | 'failed' | 'unconfirmed';

export type ReceiptState = 'delivered' | 'queued' | 'failed';

const QUEUED_WHY = ['turn', 'dialog', 'prompt', 'draft'];

export const isQueuedWhy = (why: string | undefined): boolean =>
	!!why && QUEUED_WHY.includes(why);

export interface Receipt {
	id: string;
	state: ReceiptState;
	reason?: string;
}

export interface Delivery {
	state: DeliveryState;
	remote: boolean;
	reason?: string;
	retried?: boolean;
}

export const CONFIRM_MS = 20_000;
export const SENDING_MS = 30_000;
export const SEND_TIMEOUT_REASON = 'send_timeout';
export const MAX_TRACKED = 200;

export const startDelivery = (remote: boolean): Delivery => ({
	state: 'sending',
	remote,
});

export const onAccepted = (d: Delivery): Delivery =>
	d.state === 'sending' ? { ...d, state: 'sent' } : d;

export const onRejected = (d: Delivery, reason: string): Delivery => ({
	...d,
	state: 'failed',
	reason,
});

export const onReceipt = (
	d: Delivery,
	state: ReceiptState,
	reason?: string,
): Delivery => {
	const next: Delivery = { ...d, state };
	if (state === 'failed' || (state === 'queued' && reason))
		next.reason = reason;
	else delete next.reason;
	return next;
};

export const onSendTimeout = (d: Delivery): Delivery =>
	d.state === 'sending'
		? { ...d, state: 'failed', reason: SEND_TIMEOUT_REASON }
		: d;

export const onConfirmTimeout = (d: Delivery): Delivery =>
	d.state === 'sent' && d.remote ? { ...d, state: 'unconfirmed' } : d;

export const markRetried = (d: Delivery): Delivery => ({ ...d, retried: true });

export const canRetry = (d: Delivery | undefined): boolean =>
	!!d && !d.retried && (d.state === 'failed' || d.state === 'unconfirmed');

const isFinished = (d: Delivery): boolean =>
	d.state === 'delivered' || d.state === 'failed' || !!d.retried;

const pickVictim = (items: Map<string, Delivery>): string => {
	for (const [id, d] of items) if (isFinished(d)) return id;
	return items.keys().next().value as string;
};

export interface Timers {
	set: (fn: () => void, ms: number) => unknown;
	clear: (handle: unknown) => void;
}

const globalTimers: Timers = {
	set: (fn, ms) => setTimeout(fn, ms),
	clear: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
};

export interface DeliveryTracker {
	begin(id: string, remote: boolean): void;
	accepted(id: string): void;
	rejected(id: string, reason: string): void;
	receipt(id: string, state: ReceiptState, reason?: string): void;
	retried(id: string): void;
	rename(from: string, to: string): void;
	forget(id: string): void;
	get(id: string): Delivery | undefined;
}

export const createDeliveryTracker = (
	onChange: (id: string, d: Delivery | undefined) => void,
	timers: Timers = globalTimers,
	confirmMs: number = CONFIRM_MS,
	sendingMs: number = SENDING_MS,
): DeliveryTracker => {
	const items = new Map<string, Delivery>();
	const handles = new Map<string, unknown>();

	const disarm = (id: string) => {
		if (!handles.has(id)) return;
		timers.clear(handles.get(id));
		handles.delete(id);
	};

	const put = (id: string, d: Delivery) => {
		if (!items.has(id) && items.size >= MAX_TRACKED) {
			const victim = pickVictim(items);
			disarm(victim);
			items.delete(victim);
			onChange(victim, undefined);
		}
		items.set(id, d);
		onChange(id, d);
	};

	const update = (id: string, fn: (d: Delivery) => Delivery) => {
		const cur = items.get(id);
		if (!cur) return;
		const next = fn(cur);
		if (next !== cur) put(id, next);
	};

	return {
		begin(id, remote) {
			disarm(id);
			put(id, startDelivery(remote));
			handles.set(
				id,
				timers.set(() => {
					handles.delete(id);
					update(id, onSendTimeout);
				}, sendingMs),
			);
		},
		accepted(id) {
			const cur = items.get(id);
			if (!cur) return;
			if (!cur.remote) {
				disarm(id);
				items.delete(id);
				onChange(id, undefined);
				return;
			}
			update(id, onAccepted);
			if (items.get(id)?.state === 'sent') {
				disarm(id);
				handles.set(
					id,
					timers.set(() => {
						handles.delete(id);
						update(id, onConfirmTimeout);
					}, confirmMs),
				);
			}
		},
		rejected(id, reason) {
			disarm(id);
			update(id, (d) => onRejected(d, reason));
		},
		receipt(id, state, reason) {
			disarm(id);
			update(id, (d) => onReceipt(d, state, reason));
		},
		retried(id) {
			update(id, markRetried);
		},
		rename(from, to) {
			const cur = items.get(from);
			if (!cur) return;
			disarm(from);
			items.delete(from);
			onChange(from, undefined);
			put(to, cur);
		},
		forget(id) {
			disarm(id);
			if (items.delete(id)) onChange(id, undefined);
		},
		get: (id) => items.get(id),
	};
};

export const failureReason = (body: string): string => {
	const i = body.indexOf(':');
	return i < 0 ? '' : body.slice(i + 1).trim();
};

export const isFailureNote = (body: string): boolean =>
	body.startsWith('Not delivered');

export const newMessageId = (): string => {
	const bytes = new Uint8Array(16);
	crypto.getRandomValues(bytes);
	return `m_${Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join(
		'',
	)}`;
};
