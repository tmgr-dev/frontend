import { createLeader, type LockManagerLike } from '../leader';

class FakeLocks implements LockManagerLike {
	private held = new Set<string>();
	private queue: { name: string; run: () => void }[] = [];

	request(
		name: string,
		options: { signal?: AbortSignal; ifAvailable?: boolean },
		callback: (lock: unknown) => Promise<void>,
	) {
		return new Promise<void>((resolve, reject) => {
			const finish = () => {
				this.held.delete(name);
				const next = this.queue.findIndex((q) => q.name === name);
				if (next >= 0) this.queue.splice(next, 1)[0].run();
				resolve();
			};
			const run = () => {
				this.held.add(name);
				void callback({ name }).then(finish);
			};
			if (!this.held.has(name)) return run();
			if (options.ifAvailable) {
				void callback(null).then(() => resolve());
				return;
			}
			this.queue.push({ name, run });
			options.signal?.addEventListener('abort', () => {
				const index = this.queue.findIndex((q) => q.run === run);
				if (index >= 0) {
					this.queue.splice(index, 1);
					reject(new Error('aborted'));
				}
			});
		});
	}
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

it('leads at once without a lock manager', async () => {
	expect(await createLeader(null).lead('a', () => undefined)).toBe(true);
});

it('lets the first tab lead and promotes the next when it leaves', async () => {
	const locks = new FakeLocks();
	const first = createLeader(locks);
	const second = createLeader(locks);
	expect(await first.lead('a', () => undefined)).toBe(true);
	const promoted = jest.fn();
	expect(await second.lead('a', promoted)).toBe(false);
	expect(promoted).not.toHaveBeenCalled();
	first.leave();
	await flush();
	expect(promoted).toHaveBeenCalledTimes(1);
});

it('does not promote a tab that moved to another name', async () => {
	const locks = new FakeLocks();
	const first = createLeader(locks);
	const second = createLeader(locks);
	await first.lead('a', () => undefined);
	const promoted = jest.fn();
	await second.lead('a', promoted);
	expect(await second.lead('b', () => undefined)).toBe(true);
	first.leave();
	await flush();
	expect(promoted).not.toHaveBeenCalled();
});
