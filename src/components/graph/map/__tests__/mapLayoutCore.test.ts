import { MapLayoutCore, type LayoutInit } from '../mapLayoutCore';

const build = (perCluster: number, group = true): LayoutInit => {
	const clusters = 3;
	const count = perCluster * clusters;
	const cluster = new Int32Array(count);
	const links: number[] = [];
	const cross: number[] = [];
	for (let i = 0; i < count; i++) {
		cluster[i] = Math.floor(i / perCluster);
		if (i % perCluster !== 0) {
			links.push(cluster[i] * perCluster, i);
			cross.push(0);
		}
	}
	links.push(0, perCluster);
	cross.push(1);
	return {
		count,
		cluster,
		radius: new Float32Array(count).fill(3),
		orphan: new Uint8Array(count),
		links: Int32Array.from(links),
		cross: Uint8Array.from(cross),
		centers: Float32Array.from([-300, 0, 300, 0, 0, 400]),
		clusterRadius: Float32Array.from([120, 120, 120]),
		ring: 600,
		seed: new Float32Array(count).fill(0.25),
		group,
	};
};

describe('MapLayoutCore', () => {
	it('settles, keeps every node finite and pulls nodes to their cluster', () => {
		const init = build(30);
		const pos = new MapLayoutCore(init).settle();
		expect(pos).toHaveLength(init.count * 2);
		expect(pos.every(Number.isFinite)).toBe(true);
		for (let i = 0; i < init.count; i++) {
			const c = init.cluster[i];
			const d = Math.hypot(
				pos[i * 2] - init.centers[c * 2],
				pos[i * 2 + 1] - init.centers[c * 2 + 1],
			);
			expect(d).toBeLessThan(init.clusterRadius[c] * 1.4);
		}
	});

	it('is deterministic', () => {
		const a = new MapLayoutCore(build(20)).settle();
		const b = new MapLayoutCore(build(20)).settle();
		expect(Array.from(a)).toEqual(Array.from(b));
	});

	it('reports progress in slices and finishes', () => {
		const core = new MapLayoutCore(build(20));
		let last = core.step(0);
		expect(last.positions).toHaveLength(120);
		let guard = 0;
		while (!last.done && guard++ < 1000) last = core.step(0);
		expect(last.done).toBe(true);
	});

	it('puts unlinked items on the outer ring', () => {
		const init = build(10);
		init.orphan[5] = 1;
		const pos = new MapLayoutCore(init).settle();
		expect(Math.hypot(pos[10], pos[11])).toBeGreaterThan(init.ring * 0.6);
	});

	it('lays out differently when grouping is off', () => {
		const grouped = new MapLayoutCore(build(30, true)).settle();
		const loose = new MapLayoutCore(build(30, false)).settle();
		const spread = (p: Float32Array) => {
			let s = 0;
			for (let i = 1; i < 30; i++)
				s += Math.hypot(p[i * 2] - p[0], p[i * 2 + 1] - p[1]);
			return s / 29;
		};
		expect(Math.abs(spread(grouped) - spread(loose))).toBeGreaterThan(0);
	});
});
