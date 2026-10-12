import { createLease, LEASE_KEY } from '../lease';

const memory = () => {
	const data = new Map<string, string>();
	return {
		getItem: (k: string) => data.get(k) ?? null,
		setItem: (k: string, v: string) => void data.set(k, v),
		removeItem: (k: string) => void data.delete(k),
	};
};

describe('lease', () => {
	it('blocks a second tab until expiry, then hands over', () => {
		const storage = memory();
		let t = 1000;
		const a = createLease({ storage, now: () => t, ownerId: 'a' });
		const b = createLease({ storage, now: () => t, ownerId: 'b' });
		expect(a.acquire()).toBe(true);
		expect(b.acquire()).toBe(false);
		expect(a.acquire()).toBe(true);
		t += 29_000;
		expect(b.acquire()).toBe(false);
		t += 2_000;
		expect(b.acquire()).toBe(true);
		expect(a.holds()).toBe(false);
		expect(a.renew()).toBe(false);
	});

	it('renew extends the expiry', () => {
		const storage = memory();
		let t = 0;
		const a = createLease({ storage, now: () => t, ownerId: 'a' });
		const b = createLease({ storage, now: () => t, ownerId: 'b' });
		a.acquire();
		t = 20_000;
		expect(a.renew()).toBe(true);
		t = 40_000;
		expect(b.acquire()).toBe(false);
	});

	it('release frees only own lease', () => {
		const storage = memory();
		const a = createLease({ storage, now: () => 0, ownerId: 'a' });
		const b = createLease({ storage, now: () => 0, ownerId: 'b' });
		a.acquire();
		b.release();
		expect(storage.getItem(LEASE_KEY)).not.toBeNull();
		a.release();
		expect(b.acquire()).toBe(true);
	});

	it('tolerates corrupt storage and broadcasts through the channel', () => {
		const storage = memory();
		storage.setItem(LEASE_KEY, '{nope');
		const posted: unknown[] = [];
		const channel = { postMessage: (m: unknown) => void posted.push(m), onmessage: null as any };
		const a = createLease({ storage, channel, now: () => 0, ownerId: 'a' });
		expect(a.acquire()).toBe(true);
		a.broadcast({ type: 'x' });
		expect(posted).toEqual([{ type: 'x' }]);
		const seen: unknown[] = [];
		a.onMessage((m) => seen.push(m));
		channel.onmessage({ data: { type: 'y' } });
		channel.onmessage({ data: 'junk' });
		expect(seen).toEqual([{ type: 'y' }]);
	});
});
