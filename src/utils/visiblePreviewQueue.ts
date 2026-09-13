type Observer = Pick<
	IntersectionObserver,
	'observe' | 'unobserve' | 'disconnect'
>;
type Visibility = (element: Element, visible: boolean) => void;
const browserObserver = (visible: Visibility): Observer | undefined => {
	if (typeof IntersectionObserver === 'undefined') return;
	return new IntersectionObserver(
		(entries) => {
			entries.forEach((entry) => visible(entry.target, entry.isIntersecting));
		},
		{ rootMargin: '200px' },
	);
};

/** Defer preview requests until their rows approach the viewport; retain active slots across resets. */
export function createVisiblePreviewQueue(
	options: {
		limit?: number;
		createObserver?: (visible: Visibility) => Observer | undefined;
	} = {},
) {
	const limit = options.limit ?? 4;
	if (!Number.isInteger(limit) || limit < 1)
		throw new RangeError('Concurrency must be positive');
	const entries = new Map<
		number,
		{
			element: Element;
			visible: boolean;
			started: boolean;
			work: () => Promise<unknown>;
		}
	>();
	const elements = new Map<Element, number>();
	let active = 0,
		disposed = false;
	const drain = () => {
		if (disposed) return;
		for (const [key, entry] of entries) {
			if (active >= limit) break;
			if (!entry.visible || entry.started) continue;
			entry.started = true;
			active++;
			// Claim the slot synchronously; execute only if the row still belongs to this list.
			Promise.resolve()
				.then(() => {
					if (!disposed && entries.get(key) === entry) return entry.work();
				})
				.catch(() => undefined)
				.finally(() => {
					active--;
					drain();
				});
		}
	};
	const observer = (options.createObserver ?? browserObserver)(
		(element, visible) => {
			const key = elements.get(element);
			const entry = key === undefined ? undefined : entries.get(key);
			if (!entry || disposed) return;
			entry.visible = visible;
			drain();
		},
	);
	const reset = () => {
		observer?.disconnect();
		entries.clear();
		elements.clear();
	};
	return {
		bind(key: number, element: Element | null, work: () => Promise<unknown>) {
			if (disposed) return;
			const old = entries.get(key);
			if (old?.element === element) return;
			if (old) {
				observer?.unobserve(old.element);
				elements.delete(old.element);
				entries.delete(key);
			}
			if (!element) return;
			entries.set(key, { element, work, visible: !observer, started: false });
			elements.set(element, key);
			observer?.observe(element);
			drain();
		},
		reset,
		dispose() {
			disposed = true;
			reset();
		},
	};
}
