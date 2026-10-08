import {
	forceCollide,
	forceLink,
	forceManyBody,
	forceSimulation,
	type Simulation,
	type SimulationLinkDatum,
	type SimulationNodeDatum,
} from 'd3-force';

export interface LayoutInit {
	count: number;
	cluster: Int32Array;
	radius: Float32Array;
	orphan: Uint8Array;
	links: Int32Array;
	cross: Uint8Array;
	centers: Float32Array;
	clusterRadius: Float32Array;
	ring: number;
	seed: Float32Array;
	group: boolean;
}

export interface LayoutStep {
	positions: Float32Array;
	done: boolean;
	ticks: number;
}

interface LayoutNode extends SimulationNodeDatum {
	i: number;
}

interface LayoutLink extends SimulationLinkDatum<LayoutNode> {
	cross: boolean;
}

const GOLDEN = 2.399963229728653;
const GROUP_PULL = 0.06;
const LOOSE_PULL = 0.012;
const ALPHA_MIN = 0.02;

export class MapLayoutCore {
	private nodes: LayoutNode[];
	private sim: Simulation<LayoutNode, LayoutLink>;
	private pull: number;
	private ticks = 0;

	constructor(private init: LayoutInit) {
		this.pull = init.group ? GROUP_PULL : LOOSE_PULL;
		const perCluster = new Map<number, number>();
		this.nodes = Array.from({ length: init.count }, (_, i) => {
			const c = init.cluster[i];
			if (init.orphan[i]) {
				const a = init.seed[i] * Math.PI * 2;
				const d = init.ring * (0.9 + (0.2 * ((i * 7919) % 100)) / 100);
				return { i, x: Math.cos(a) * d, y: Math.sin(a) * d };
			}
			const slot = perCluster.get(c) ?? 0;
			perCluster.set(c, slot + 1);
			const r = init.clusterRadius[c] * 0.8 * Math.sqrt((slot + 0.5) / 40);
			const a = slot * GOLDEN;
			return {
				i,
				x:
					init.centers[c * 2] +
					Math.cos(a) * Math.min(r, init.clusterRadius[c]),
				y:
					init.centers[c * 2 + 1] +
					Math.sin(a) * Math.min(r, init.clusterRadius[c]),
			};
		});
		const links: LayoutLink[] = [];
		for (let k = 0; k < init.cross.length; k++) {
			links.push({
				source: init.links[k * 2],
				target: init.links[k * 2 + 1],
				cross: init.cross[k] === 1,
			});
		}
		this.sim = forceSimulation<LayoutNode, LayoutLink>(this.nodes)
			.alphaDecay(0.045)
			.alphaMin(ALPHA_MIN)
			.velocityDecay(0.5)
			.force(
				'link',
				forceLink<LayoutNode, LayoutLink>(links)
					.id((d) => d.i)
					.distance((l) => (l.cross ? 160 : 36))
					.strength((l) => (l.cross ? 0.01 : 0.14))
					.iterations(1),
			)
			.force(
				'charge',
				forceManyBody<LayoutNode>()
					.strength((d) => (init.orphan[d.i] ? -2 : -38))
					.theta(1)
					.distanceMax(150),
			)
			.force(
				'collide',
				forceCollide<LayoutNode>()
					.radius((d) => init.radius[d.i] + 3)
					.strength(0.7)
					.iterations(1),
			)
			.force('cluster', this.clusterForce())
			.stop();
	}

	private clusterForce() {
		const init = this.init;
		const nodes = this.nodes;
		return (alpha: number) => {
			const k = this.pull * alpha;
			for (let n = 0; n < nodes.length; n++) {
				const node = nodes[n];
				const i = node.i;
				const x = node.x ?? 0;
				const y = node.y ?? 0;
				if (init.orphan[i]) {
					const d = Math.hypot(x, y) || 1;
					const pushR = (init.ring - d) * 0.06 * alpha;
					node.vx = (node.vx ?? 0) + (x / d) * pushR * 4;
					node.vy = (node.vy ?? 0) + (y / d) * pushR * 4;
					continue;
				}
				const c = init.cluster[i];
				const cx = init.centers[c * 2];
				const cy = init.centers[c * 2 + 1];
				node.vx = (node.vx ?? 0) + (cx - x) * k;
				node.vy = (node.vy ?? 0) + (cy - y) * k;
				if (this.pull === GROUP_PULL) {
					const dist = Math.hypot(x - cx, y - cy);
					const limit = init.clusterRadius[c] * 0.95;
					if (dist > limit) {
						const over = ((dist - limit) / dist) * 0.5 * alpha * 2;
						node.vx = (node.vx ?? 0) - (x - cx) * over;
						node.vy = (node.vy ?? 0) - (y - cy) * over;
					}
				}
			}
		};
	}

	setGroup(group: boolean) {
		this.pull = group ? GROUP_PULL : LOOSE_PULL;
		this.sim.alpha(0.8);
	}

	step(budgetMs: number): LayoutStep {
		const stop = performance.now() + budgetMs;
		let ran = 0;
		while (this.sim.alpha() >= ALPHA_MIN) {
			this.sim.tick();
			ran++;
			if (performance.now() >= stop) break;
		}
		this.ticks += ran;
		const positions = new Float32Array(this.nodes.length * 2);
		for (let n = 0; n < this.nodes.length; n++) {
			positions[n * 2] = this.nodes[n].x ?? 0;
			positions[n * 2 + 1] = this.nodes[n].y ?? 0;
		}
		return {
			positions,
			done: this.sim.alpha() < ALPHA_MIN,
			ticks: this.ticks,
		};
	}

	settle(): Float32Array {
		let last: LayoutStep;
		do {
			last = this.step(1000);
		} while (!last.done);
		return last.positions;
	}
}
