import { createVisiblePreviewQueue } from '../visiblePreviewQueue';
const flush = async () => {
	for (let i = 0; i < 8; i++) await Promise.resolve();
};
function fixture(limit = 4) {
	let visibility!: (element: Element, visible: boolean) => void;
	const observer = {
		observe: jest.fn(),
		unobserve: jest.fn(),
		disconnect: jest.fn(),
	};
	const queue = createVisiblePreviewQueue({
		limit,
		createObserver: (callback) => {
			visibility = callback;
			return observer;
		},
	});
	const elements = Array.from({ length: 10 }, () => ({} as Element));
	const releases: Array<() => void> = [];
	const started: number[] = [];
	const bind = (id: number) =>
		queue.bind(id, elements[id], () => {
			started.push(id);
			return new Promise<void>((resolve) => releases.push(resolve));
		});
	return {
		queue,
		observer,
		elements,
		releases,
		started,
		bind,
		visible: (id: number, value = true) => visibility(elements[id], value),
	};
}
test('starts only visible rows, including a later row before offscreen predecessors', async () => {
	const f = fixture();
	f.bind(0);
	f.bind(1);
	f.bind(2);
	await flush();
	expect(f.started).toEqual([]);
	f.visible(2);
	await flush();
	expect(f.started).toEqual([2]);
	f.visible(0);
	await flush();
	expect(f.started).toEqual([2, 0]);
	f.queue.dispose();
	f.releases.forEach((done) => done());
	await flush();
});
test('never exceeds four active previews, including after list reset', async () => {
	const f = fixture();
	for (let id = 0; id < 6; id++) {
		f.bind(id);
		f.visible(id);
	}
	await flush();
	expect(f.started).toEqual([0, 1, 2, 3]);
	f.queue.reset();
	f.bind(6);
	f.visible(6);
	await flush();
	expect(f.started).toHaveLength(4);
	f.releases[0]();
	await flush();
	expect(f.started).toEqual([0, 1, 2, 3, 6]);
	f.queue.dispose();
	f.releases.forEach((done) => done());
	await flush();
});
test('disposal disconnects observation and late completion cannot start queued work', async () => {
	const f = fixture(1);
	f.bind(0);
	f.bind(1);
	f.visible(0);
	f.visible(1);
	await flush();
	f.queue.dispose();
	f.releases[0]();
	await flush();
	f.visible(1);
	f.bind(2);
	await flush();
	expect(f.started).toEqual([0]);
	expect(f.observer.disconnect).toHaveBeenCalled();
});
test('rows leaving viewport while queued stay deferred', async () => {
	const f = fixture(1);
	f.bind(0);
	f.bind(1);
	f.visible(0);
	f.visible(1);
	f.visible(1, false);
	await flush();
	f.releases[0]();
	await flush();
	expect(f.started).toEqual([0]);
	f.visible(1);
	await flush();
	expect(f.started).toEqual([0, 1]);
	f.queue.dispose();
	f.releases.forEach((done) => done());
	await flush();
});
test('missing IntersectionObserver falls back to a bounded queue', async () => {
	let active = 0,
		maximum = 0,
		calls = 0;
	const queue = createVisiblePreviewQueue({ createObserver: () => undefined });
	for (let id = 0; id < 8; id++)
		queue.bind(id, {} as Element, async () => {
			maximum = Math.max(maximum, ++active);
			calls++;
			await Promise.resolve();
			active--;
		});
	await flush();
	expect(calls).toBe(8);
	expect(maximum).toBe(4);
	queue.dispose();
});

test('reset before a queued callback runs cannot execute old work for a reused row', async () => {
	const queue = createVisiblePreviewQueue({ createObserver: () => undefined });
	const element = {} as Element,
		old = jest.fn(async () => {}),
		fresh = jest.fn(async () => {});
	queue.bind(1, element, old);
	queue.reset();
	queue.bind(1, element, fresh);
	await flush();
	expect(old).not.toHaveBeenCalled();
	expect(fresh).toHaveBeenCalledTimes(1);
	queue.dispose();
});
test('a rejected preview releases its slot for the next visible row', async () => {
	const queue = createVisiblePreviewQueue({
		limit: 1,
		createObserver: () => undefined,
	});
	const next = jest.fn(async () => {});
	queue.bind(1, {} as Element, async () => {
		throw new Error('network');
	});
	queue.bind(2, {} as Element, next);
	await flush();
	expect(next).toHaveBeenCalledTimes(1);
	queue.dispose();
});
