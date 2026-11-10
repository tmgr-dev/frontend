import { createLeader, type LockManagerLike } from '../leader';

class FakeLocks implements LockManagerLike {
	private held = new Set<string>();
	private queue: { name: string; run: () => void }[] = [];

	request(
		name: string,
		options: { signal?: AbortSignal },
		callback: () => Promise<void>,
	) {
		return new Promise<void>((resolve, reject) => {
			const run = () => {
				this.held.add(name);
				void callback().then(() => {
					this.held.delete(name);
					const next = this.queue.findIndex((q) => q.name === name);
					if (next >= 0) this.queue.splice(next, 1)[0].run();
					resolve();
				});
			};
			options.signal?.addEventListener('abort', () => {
				const index = this.queue.findIndex((q) => q.run === run);
				if (index >= 0) {
					this.queue.splice(index, 1);
					reject(new Error('aborted'));
				}
			});
			if (this.held.has(name)) this.queue.push({ name, run });
			else run();
		});
	}
}

it('leads at once without a lock manager', async () => {
	expect(await createLeader(null).lead('a')).toBe(true);
});

it('lets one tab lead and the next take over when it leaves', async () => {
	const locks = new FakeLocks();
	const first = createLeader(locks);
	const second = createLeader(locks);
	expect(await first.lead('a')).toBe(true);
	let secondLeads = false;
	const waiting = second.lead('a').then((ok) => (secondLeads = ok));
	await Promise.resolve();
	expect(secondLeads).toBe(false);
	first.leave();
	expect(await waiting).toBe(true);
});

it('stops waiting when the tab moves to another name', async () => {
	const locks = new FakeLocks();
	const first = createLeader(locks);
	const second = createLeader(locks);
	await first.lead('a');
	const waiting = second.lead('a');
	expect(await second.lead('b')).toBe(true);
	expect(await waiting).toBe(false);
});
