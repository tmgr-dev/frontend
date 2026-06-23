import { createKeyedSerializer } from '../keyedSerializer';

const delay = (ms: number) =>
	new Promise<void>((resolve) => setTimeout(resolve, ms));

describe('createKeyedSerializer', () => {
	it('runs same-key tasks in submission order even when the first is slower', async () => {
		const serialize = createKeyedSerializer();
		const events: string[] = [];

		// Task 1 is slow, task 2 is instant. Without serialization task 2 would
		// finish first; serialization must keep click order (1 then 2). This is
		// the same-emoji stateful-toggle race.
		const p1 = serialize('k', async () => {
			await delay(20);
			events.push('1');
		});
		const p2 = serialize('k', async () => {
			events.push('2');
		});

		await Promise.all([p1, p2]);
		expect(events).toEqual(['1', '2']);
	});

	it('lets different-key tasks overlap (no cross-key blocking)', async () => {
		const serialize = createKeyedSerializer();
		const events: string[] = [];

		const pa = serialize('a', async () => {
			await delay(20);
			events.push('a');
		});
		const pb = serialize('b', async () => {
			events.push('b');
		});

		await Promise.all([pa, pb]);
		// b (fast, different key) is not blocked by slow a.
		expect(events).toEqual(['b', 'a']);
	});

	it('continues the chain after a task rejects', async () => {
		const serialize = createKeyedSerializer();
		const events: string[] = [];

		const p1 = serialize('k', async () => {
			events.push('1');
			throw new Error('boom');
		});
		const p2 = serialize('k', async () => {
			events.push('2');
		});

		await Promise.allSettled([p1, p2]);
		expect(events).toEqual(['1', '2']);
	});
});
