import { createPollingLoop } from '../pollingLoop';
jest.useFakeTimers();
it('never overlaps and resumes once after visibility returns', async () => {
	let change!: () => void;
	const visibility = {
		hidden: false,
		addEventListener: jest.fn((_, callback) => {
			change = callback;
		}),
		removeEventListener: jest.fn(),
	} as any;
	let release!: () => void;
	const fetch = jest.fn(
		() =>
			new Promise<void>((resolve) => {
				release = resolve;
			}),
	);
	const loop = createPollingLoop(fetch, 100, visibility);
	loop.start();
	await Promise.resolve();
	jest.advanceTimersByTime(1000);
	void loop.run();
	expect(fetch).toHaveBeenCalledTimes(1);
	visibility.hidden = true;
	change();
	release();
	await loop.run();
	jest.advanceTimersByTime(1000);
	expect(fetch).toHaveBeenCalledTimes(1);
	visibility.hidden = false;
	change();
	await Promise.resolve();
	await Promise.resolve();
	await Promise.resolve();
	loop.stop();
	release();
	await Promise.resolve();
	jest.advanceTimersByTime(1000);
	expect(fetch.mock.calls.length).toBeLessThanOrEqual(2);
	expect(visibility.removeEventListener).toHaveBeenCalledTimes(1);
});
