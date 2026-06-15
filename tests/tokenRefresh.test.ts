import { singleFlight } from '../src/plugins/tokenRefresh';

describe('singleFlight', () => {
	it('runs the worker once for concurrent callers and shares the result', async () => {
		let calls = 0;
		const worker = () =>
			new Promise<string>((resolve) => {
				calls += 1;
				setTimeout(() => resolve('new-token'), 10);
			});

		const flight = singleFlight<string>();
		const [a, b, c] = await Promise.all([
			flight(worker),
			flight(worker),
			flight(worker),
		]);

		expect(calls).toBe(1);
		expect(a).toBe('new-token');
		expect(b).toBe('new-token');
		expect(c).toBe('new-token');
	});

	it('allows a fresh run after the previous flight settles', async () => {
		let calls = 0;
		const worker = async () => {
			calls += 1;
			return `token-${calls}`;
		};
		const flight = singleFlight<string>();

		const first = await flight(worker);
		const second = await flight(worker);

		expect(first).toBe('token-1');
		expect(second).toBe('token-2');
		expect(calls).toBe(2);
	});

	it('clears in-flight state when the worker rejects, so the next call retries', async () => {
		let calls = 0;
		const worker = async () => {
			calls += 1;
			if (calls === 1) throw new Error('boom');
			return 'ok';
		};
		const flight = singleFlight<string>();

		await expect(flight(worker)).rejects.toThrow('boom');
		await expect(flight(worker)).resolves.toBe('ok');
		expect(calls).toBe(2);
	});
});
