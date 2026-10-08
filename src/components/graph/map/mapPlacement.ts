export interface ClusterPlacement {
	x: number;
	y: number;
	r: number;
}

export const clusterRadius = (count: number): number =>
	48 + 15 * Math.sqrt(Math.max(count, 1));

export const placeClusters = (counts: number[]): ClusterPlacement[] => {
	const n = counts.length;
	if (!n) return [];
	const radii = counts.map(clusterRadius);
	const mean = radii.reduce((a, b) => a + b, 0) / n;
	const golden = 2.399963229728653;
	const out = radii.map((r, i) => {
		const d = n === 1 ? 0 : mean * 1.35 * Math.sqrt(i + 0.6);
		return { x: Math.cos(i * golden) * d, y: Math.sin(i * golden) * d, r };
	});
	const gap = 70;
	const passes = Math.max(4, Math.min(120, Math.floor(4e6 / (n * n))));
	for (let pass = 0; pass < passes; pass++) {
		let moved = false;
		for (let i = 0; i < n; i++) {
			for (let j = i + 1; j < n; j++) {
				const dx = out[j].x - out[i].x;
				const dy = out[j].y - out[i].y;
				const dist = Math.hypot(dx, dy) || 0.01;
				const need = out[i].r + out[j].r + gap;
				if (dist >= need) continue;
				const push = (need - dist) / 2;
				const ux = dx / dist;
				const uy = dy / dist;
				out[i].x -= ux * push;
				out[i].y -= uy * push;
				out[j].x += ux * push;
				out[j].y += uy * push;
				moved = true;
			}
		}
		if (!moved) break;
	}
	let wx = 0;
	let wy = 0;
	let wt = 0;
	out.forEach((p) => {
		wx += p.x * p.r;
		wy += p.y * p.r;
		wt += p.r;
	});
	out.forEach((p) => {
		p.x -= wx / wt;
		p.y -= wy / wt;
	});
	return out;
};

export const orphanRingRadius = (placements: ClusterPlacement[]): number =>
	placements.reduce((m, p) => Math.max(m, Math.hypot(p.x, p.y) + p.r), 0) + 50;
