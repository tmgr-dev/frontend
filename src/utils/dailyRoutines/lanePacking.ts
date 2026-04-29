// Port of clusterEvents + packLanes from daily-routines-data.jsx.

export interface PositionedEvent<T = any> {
	startMin: number;
	endMin: number;
	lane?: number;
	lanes?: number;
	source: T;
}

export type ClusterItem<T = any> =
	| { kind: 'single'; event: PositionedEvent<T> }
	| { kind: 'cluster'; events: PositionedEvent<T>[]; startMin: number; endMin: number };

export interface MinutesEvent {
	time: { h: number; m: number } | null;
	durationMin?: number | null;
}

export function enrich<T extends MinutesEvent>(events: T[]): PositionedEvent<T>[] {
	return events
		.filter(e => e.time)
		.map(e => {
			const startMin = (e.time!.h * 60) + e.time!.m;
			const endMin = startMin + Math.max(15, e.durationMin || 30);
			return { startMin, endMin, source: e };
		})
		.sort((a, b) => a.startMin - b.startMin || a.endMin - b.endMin);
}

export function clusterEvents<T extends MinutesEvent>(events: T[]): ClusterItem<T>[] {
	const enriched = enrich(events);
	const out: ClusterItem<T>[] = [];
	let bucket: PositionedEvent<T>[] = [];
	let bucketEnd = -1;

	function flush() {
		if (bucket.length === 0) return;
		if (bucket.length === 1) {
			out.push({ kind: 'single', event: bucket[0] });
		} else {
			const startMin = Math.min(...bucket.map(e => e.startMin));
			const endMin = Math.max(...bucket.map(e => e.endMin));
			out.push({ kind: 'cluster', events: bucket.slice(), startMin, endMin });
		}
		bucket = [];
		bucketEnd = -1;
	}

	for (const ev of enriched) {
		if (bucket.length === 0 || ev.startMin < bucketEnd) {
			bucket.push(ev);
			bucketEnd = Math.max(bucketEnd, ev.endMin);
		} else {
			flush();
			bucket.push(ev);
			bucketEnd = ev.endMin;
		}
	}
	flush();
	return out;
}

export function packLanes<T extends MinutesEvent>(events: T[]): PositionedEvent<T>[] {
	const enriched = enrich(events);
	const out: PositionedEvent<T>[] = [];
	let cluster: PositionedEvent<T>[] = [];
	let clusterEnd = -1;

	function flush() {
		if (cluster.length === 0) return;
		const laneEnds: number[] = [];
		for (const ev of cluster) {
			let lane = laneEnds.findIndex(end => end <= ev.startMin);
			if (lane === -1) {
				lane = laneEnds.length;
				laneEnds.push(0);
			}
			laneEnds[lane] = ev.endMin;
			ev.lane = lane;
		}
		const lanes = laneEnds.length;
		for (const ev of cluster) {
			ev.lanes = lanes;
			out.push(ev);
		}
		cluster = [];
		clusterEnd = -1;
	}

	for (const ev of enriched) {
		if (cluster.length === 0 || ev.startMin < clusterEnd) {
			cluster.push(ev);
			clusterEnd = Math.max(clusterEnd, ev.endMin);
		} else {
			flush();
			cluster.push(ev);
			clusterEnd = ev.endMin;
		}
	}
	flush();
	return out;
}
