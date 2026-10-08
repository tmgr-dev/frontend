import type {
	GraphEdge,
	GraphNode,
	GraphNodeType,
	GraphResult,
} from '@/types/graph';
import {
	forceCollide,
	forceLink,
	forceManyBody,
	forceSimulation,
	type Simulation,
	type SimulationLinkDatum,
	type SimulationNodeDatum,
} from 'd3-force';
import {
	curveControl,
	edgeBend,
	edgeColorType,
	fitTransform,
	isExpandable,
	layoutShape,
	linkDistance,
	neighbourhood,
	nodeRadius,
	quadPoint,
	readThemeFromDom,
	RING_1,
	RING_2,
	ringTarget,
	seedPositions,
	truncateLabel,
	TYPE_STYLES,
	type GraphShape,
	type GraphTheme,
	type LayoutShape,
	type Neighbourhood,
	type Transform,
} from './graphLogic';

interface SimNode extends SimulationNodeDatum {
	id: string;
	data: GraphNode;
	r: number;
	a: number;
	ta: number;
	dim: number;
	tdim: number;
	leaving: boolean;
}

interface SimLink extends SimulationLinkDatum<SimNode> {
	edge: GraphEdge;
	bend: number;
	ea: number;
	tea: number;
	pulse: number;
}

export interface EngineOptions {
	compact: boolean;
	wheelZoom: 'always' | 'modifier';
	reducedMotion: boolean;
}

export interface EngineHandlers {
	onSelect: (id: string | null) => void;
	onCenter: (id: string) => void;
	onHover: (id: string | null) => void;
}

interface LabelRect {
	x: number;
	y: number;
	w: number;
	h: number;
}

const MIN_K = 0.15;
const MAX_K = 4;
const TICK_MS = 1000 / 60;
const GLOW_NODE_LIMIT = 260;

const overlaps = (a: LabelRect, list: LabelRect[]): boolean =>
	list.some(
		(b) =>
			a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y,
	);

export class GraphEngine {
	private ctx: CanvasRenderingContext2D;
	private sim: Simulation<SimNode, SimLink>;
	private linkForce = forceLink<SimNode, SimLink>([]);
	private nodes: SimNode[] = [];
	private links: SimLink[] = [];
	private byId = new Map<string, SimNode>();
	private center = '';
	private theme: GraphTheme;
	private shape: LayoutShape = { ax: 1, ay: 1 };
	private width = 0;
	private height = 0;
	private dpr = 1;
	private view: Transform = { k: 1, x: 0, y: 0 };
	private autoFit = true;
	private raf = 0;
	private lastTime = 0;
	private tickDebt = 0;
	private hoverId: string | null = null;
	private selectedId: string | null = null;
	private hood: Neighbourhood | null = null;
	private sprites = new Map<string, HTMLCanvasElement>();
	private observer: ResizeObserver | null = null;
	private pointers = new Map<number, { x: number; y: number }>();
	private down: {
		x: number;
		y: number;
		node: SimNode | null;
		moved: boolean;
	} | null = null;
	private pinch: { dist: number } | null = null;
	private destroyed = false;
	private ringNodes: SimNode[] = [];
	private topInset = 0;

	constructor(
		private canvas: HTMLCanvasElement,
		private options: EngineOptions,
		private handlers: EngineHandlers,
	) {
		this.ctx = canvas.getContext('2d')!;
		this.theme = readThemeFromDom(canvas);
		this.sim = forceSimulation<SimNode, SimLink>()
			.alphaDecay(0.032)
			.velocityDecay(0.42)
			.stop();
		this.linkForce
			.id((d) => d.id)
			.distance((l) => {
				const s = l.source as SimNode;
				const t = l.target as SimNode;
				return linkDistance(s.data.hop, t.data.hop, l.edge.weight);
			})
			.strength(0.2);
		this.sim
			.force('link', this.linkForce)
			.force(
				'charge',
				forceManyBody<SimNode>()
					.strength((d) =>
						d.data.hop === 0 ? -420 : d.data.hop === 1 ? -230 : -110,
					)
					.distanceMax(520),
			)
			.force(
				'collide',
				forceCollide<SimNode>()
					.radius((d) => d.r + (d.data.hop <= 1 ? 22 : 10))
					.strength(0.85),
			)
			.force('ring', this.ringForce());
		canvas.addEventListener('pointerdown', this.onPointerDown);
		canvas.addEventListener('pointermove', this.onPointerMove);
		canvas.addEventListener('pointerup', this.onPointerUp);
		canvas.addEventListener('pointercancel', this.onPointerUp);
		canvas.addEventListener('pointerleave', this.onPointerLeave);
		canvas.addEventListener('dblclick', this.onDblClick);
		canvas.addEventListener('wheel', this.onWheel, { passive: false });
		this.observer = new ResizeObserver(() => this.resize());
		this.observer.observe(canvas);
		this.resize();
	}

	private ringForce() {
		const force = (alpha: number) => {
			for (const n of this.ringNodes) {
				if (n.data.hop === 0 || n.fx != null) continue;
				const t = ringTarget(n.x ?? 0, n.y ?? 0, n.data.hop, this.shape);
				const k = (n.data.hop === 1 ? 0.62 : 0.34) * alpha;
				n.vx = (n.vx ?? 0) + (t.x - (n.x ?? 0)) * k;
				n.vy = (n.vy ?? 0) + (t.y - (n.y ?? 0)) * k;
			}
		};
		force.initialize = (nodes: SimNode[]) => {
			this.ringNodes = nodes;
		};
		return force;
	}

	setData(result: GraphResult) {
		const previous = new Map<string, { x: number; y: number }>();
		for (const n of this.nodes) {
			if (!n.leaving) previous.set(n.id, { x: n.x ?? 0, y: n.y ?? 0 });
		}
		const seeds = seedPositions(previous, result.nodes, result.edges);
		const wanted = new Set(result.nodes.map((n) => n.id));
		const firstLoad = this.nodes.length === 0;
		this.center = result.center;

		for (const n of this.nodes) {
			if (!wanted.has(n.id)) {
				n.leaving = true;
				n.ta = 0;
				n.fx = null;
				n.fy = null;
			}
		}
		for (const data of result.nodes) {
			const seed = seeds.get(data.id)!;
			let node = this.byId.get(data.id);
			if (!node) {
				node = {
					id: data.id,
					data,
					r: nodeRadius(data, this.options.compact),
					x: seed.x,
					y: seed.y,
					a: 0,
					ta: 1,
					dim: 1,
					tdim: 1,
					leaving: false,
				};
				this.byId.set(data.id, node);
				this.nodes.push(node);
			}
			node.data = data;
			node.r = nodeRadius(data, this.options.compact);
			node.leaving = false;
			node.ta = 1;
			if (data.hop === 0) {
				node.fx = 0;
				node.fy = 0;
			} else if (node.fx != null && node.id !== this.hoverId) {
				node.fx = null;
				node.fy = null;
			}
		}
		const oldLinks = new Map(this.links.map((l) => [l.edge.id, l]));
		this.links = result.edges
			.filter((e) => wanted.has(e.from) && wanted.has(e.to))
			.map((edge) => {
				const kept = oldLinks.get(edge.id);
				return {
					source: edge.from,
					target: edge.to,
					edge,
					bend: edgeBend(edge.id),
					ea: kept ? kept.ea : 0,
					tea: 1,
					pulse: (edgeBend(`${edge.id}:p`) + 0.2) * 1.6 + 0.2,
				} as SimLink;
			});
		const active = this.nodes.filter((n) => !n.leaving);
		this.sim.nodes(active);
		this.linkForce.links(this.links);
		this.hood = this.hoverId ? neighbourhood(result.edges, this.hoverId) : null;
		this.sim.alpha(firstLoad ? 1 : 0.55);
		this.autoFit = this.autoFit || firstLoad;
		if (this.options.reducedMotion) {
			this.sim.tick(320);
			for (const n of this.nodes) n.a = n.ta;
			for (const l of this.links) l.ea = l.tea;
			this.nodes = this.nodes.filter((n) => !n.leaving);
			this.byId = new Map(this.nodes.map((n) => [n.id, n]));
			this.snapView();
		}
		this.applyEmphasis();
		this.wake();
	}

	setSelected(id: string | null) {
		this.selectedId = id;
		this.wake();
	}

	refreshTheme() {
		this.theme = readThemeFromDom(this.canvas);
		this.sprites.clear();
		this.wake();
	}

	fit() {
		this.autoFit = true;
		this.wake();
	}

	zoomBy(factor: number) {
		this.zoomAt(this.width / 2, this.height / 2, factor);
	}

	destroy() {
		this.destroyed = true;
		cancelAnimationFrame(this.raf);
		this.sim.stop();
		this.observer?.disconnect();
		const c = this.canvas;
		c.removeEventListener('pointerdown', this.onPointerDown);
		c.removeEventListener('pointermove', this.onPointerMove);
		c.removeEventListener('pointerup', this.onPointerUp);
		c.removeEventListener('pointercancel', this.onPointerUp);
		c.removeEventListener('pointerleave', this.onPointerLeave);
		c.removeEventListener('dblclick', this.onDblClick);
		c.removeEventListener('wheel', this.onWheel);
	}

	private resize() {
		const rect = this.canvas.getBoundingClientRect();
		const dpr = Math.min(2, window.devicePixelRatio || 1);
		const w = Math.max(1, Math.round(rect.width));
		const h = Math.max(1, Math.round(rect.height));
		if (w === this.width && h === this.height && dpr === this.dpr) return;
		this.width = w;
		this.height = h;
		this.dpr = dpr;
		this.canvas.width = Math.round(w * dpr);
		this.canvas.height = Math.round(h * dpr);
		const next = layoutShape(w, h);
		const changed =
			Math.abs(next.ax - this.shape.ax) > 0.05 ||
			Math.abs(next.ay - this.shape.ay) > 0.05;
		this.shape = next;
		if (changed && this.nodes.length) {
			if (this.options.reducedMotion) this.sim.alpha(1).tick(320);
			else this.sim.alpha(Math.max(this.sim.alpha(), 0.5));
		}
		if (this.autoFit) this.snapView();
		this.wake();
	}

	private fitTarget(): Transform {
		const active = this.nodes.filter((n) => !n.leaving);
		if (!active.length) return { k: 1, x: this.width / 2, y: this.height / 2 };
		let minX = Infinity;
		let minY = Infinity;
		let maxX = -Infinity;
		let maxY = -Infinity;
		for (const n of active) {
			const x = n.x ?? 0;
			const y = n.y ?? 0;
			minX = Math.min(minX, x - n.r);
			maxX = Math.max(maxX, x + n.r);
			minY = Math.min(minY, y - n.r);
			maxY = Math.max(maxY, y + n.r);
		}
		const hasOuter = active.some((n) => n.data.hop === 2);
		const reach = hasOuter ? RING_2 * 0.82 : RING_1 * 1.05;
		minX = Math.min(minX, -reach * this.shape.ax * 0.7);
		maxX = Math.max(maxX, reach * this.shape.ax * 0.7);
		minY = Math.min(minY, -reach * 0.7);
		maxY = Math.max(maxY, reach * 0.7);
		const pad = Math.min(
			this.options.compact ? 44 : 70,
			Math.min(this.width, this.height) * 0.12,
		);
		const inset = Math.min(this.topInset, this.height * 0.4);
		const fit = fitTransform(
			{ minX, minY, maxX, maxY },
			this.width,
			this.height - inset,
			pad,
			this.options.compact ? 1.1 : 1.25,
		);
		return { ...fit, y: fit.y + inset };
	}

	setTopInset(px: number) {
		if (px === this.topInset) return;
		this.topInset = px;
		if (this.autoFit) this.wake();
	}

	private snapView() {
		this.view = this.fitTarget();
	}

	private wake() {
		if (this.destroyed || this.raf) return;
		this.lastTime = 0;
		this.raf = requestAnimationFrame(this.frame);
	}

	private applyEmphasis() {
		const hood = this.hood;
		for (const n of this.nodes) {
			if (n.leaving) continue;
			const base = n.data.hop >= 2 ? 0.6 : 1;
			n.tdim = hood ? (hood.nodes.has(n.id) ? 1 : 0.14) : base;
		}
		for (const l of this.links) {
			l.tea = hood ? (hood.edges.has(l.edge.id) ? 2 : 0.12) : 1;
		}
	}

	private frame = (time: number) => {
		this.raf = 0;
		if (this.destroyed) return;
		const dt = this.lastTime
			? Math.min(0.1, (time - this.lastTime) / 1000)
			: 0.016;
		this.lastTime = time;
		let busy = false;

		if (this.sim.alpha() >= this.sim.alphaMin() || this.sim.alphaTarget() > 0) {
			this.tickDebt += dt * 1000;
			let steps = 0;
			while (this.tickDebt >= TICK_MS && steps < 3) {
				this.sim.tick();
				this.tickDebt -= TICK_MS;
				steps++;
			}
			if (this.tickDebt > TICK_MS * 3) this.tickDebt = 0;
			busy = true;
		}

		const ease = 1 - Math.exp(-dt * 9);
		const easeSlow = 1 - Math.exp(-dt * 6);
		for (const n of this.nodes) {
			if (Math.abs(n.ta - n.a) > 0.01) {
				n.a += (n.ta - n.a) * easeSlow;
				busy = true;
			} else n.a = n.ta;
			if (Math.abs(n.tdim - n.dim) > 0.01) {
				n.dim += (n.tdim - n.dim) * ease;
				busy = true;
			} else n.dim = n.tdim;
		}
		for (const l of this.links) {
			if (Math.abs(l.tea - l.ea) > 0.01) {
				l.ea += (l.tea - l.ea) * ease;
				busy = true;
			} else l.ea = l.tea;
		}
		if (this.nodes.some((n) => n.leaving && n.a < 0.03)) {
			this.nodes = this.nodes.filter((n) => !(n.leaving && n.a < 0.03));
			this.byId = new Map(this.nodes.map((n) => [n.id, n]));
		}

		if (this.autoFit) {
			const target = this.fitTarget();
			const v = this.view;
			const dk = target.k - v.k;
			const dx = target.x - v.x;
			const dy = target.y - v.y;
			if (Math.abs(dk) > 0.0008 || Math.abs(dx) > 0.3 || Math.abs(dy) > 0.3) {
				v.k += dk * easeSlow;
				v.x += dx * easeSlow;
				v.y += dy * easeSlow;
				busy = true;
			} else this.view = target;
		}

		this.draw();
		if (busy || this.down?.moved) this.raf = requestAnimationFrame(this.frame);
	};

	private sx(x: number) {
		return x * this.view.k + this.view.x;
	}

	private sy(y: number) {
		return y * this.view.k + this.view.y;
	}

	private sprite(color: string): HTMLCanvasElement {
		let sprite = this.sprites.get(color);
		if (sprite) return sprite;
		sprite = document.createElement('canvas');
		sprite.width = sprite.height = 128;
		const g = sprite.getContext('2d')!;
		const grad = g.createRadialGradient(64, 64, 0, 64, 64, 64);
		grad.addColorStop(0, color);
		grad.addColorStop(0.35, color);
		grad.addColorStop(1, 'rgba(0,0,0,0)');
		g.fillStyle = grad;
		g.fillRect(0, 0, 128, 128);
		const tinted = document.createElement('canvas');
		tinted.width = tinted.height = 128;
		const t = tinted.getContext('2d')!;
		t.drawImage(sprite, 0, 0);
		t.globalCompositeOperation = 'destination-in';
		const mask = t.createRadialGradient(64, 64, 0, 64, 64, 64);
		mask.addColorStop(0, 'rgba(0,0,0,0.9)');
		mask.addColorStop(0.28, 'rgba(0,0,0,0.55)');
		mask.addColorStop(1, 'rgba(0,0,0,0)');
		t.fillStyle = mask;
		t.fillRect(0, 0, 128, 128);
		this.sprites.set(color, tinted);
		return tinted;
	}

	private draw() {
		const { ctx, theme, width, height, view } = this;
		ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
		ctx.fillStyle = theme.bg;
		ctx.fillRect(0, 0, width, height);

		const centerNode = this.byId.get(this.center);
		const cx = this.sx(centerNode?.x ?? 0);
		const cy = this.sy(centerNode?.y ?? 0);
		const reach = RING_2 * view.k * Math.max(1, this.shape.ax * 0.85);
		const halo = ctx.createRadialGradient(cx, cy, 0, cx, cy, reach);
		halo.addColorStop(0, withAlpha(theme.colors.task, theme.dark ? 0.2 : 0.13));
		halo.addColorStop(
			0.55,
			withAlpha(theme.colors.task, theme.dark ? 0.06 : 0.04),
		);
		halo.addColorStop(1, withAlpha(theme.colors.task, 0));
		ctx.fillStyle = halo;
		ctx.fillRect(0, 0, width, height);
		const vignette = ctx.createRadialGradient(
			width / 2,
			height / 2,
			Math.min(width, height) * 0.45,
			width / 2,
			height / 2,
			Math.hypot(width, height) * 0.62,
		);
		vignette.addColorStop(0, 'rgba(0,0,0,0)');
		vignette.addColorStop(
			1,
			theme.dark ? 'rgba(0,0,0,0.38)' : 'rgba(24,24,40,0.07)',
		);
		ctx.fillStyle = vignette;
		ctx.fillRect(0, 0, width, height);

		const hasOuter = this.nodes.some((n) => n.data.hop === 2 && !n.leaving);
		ctx.save();
		ctx.lineWidth = 1;
		ctx.setLineDash([2, 6]);
		ctx.strokeStyle = theme.line;
		const rings = hasOuter ? [RING_1, RING_2] : [RING_1];
		for (const r of rings) {
			ctx.globalAlpha = r === RING_1 ? 1 : 0.6;
			ctx.beginPath();
			ctx.ellipse(
				cx,
				cy,
				r * this.shape.ax * view.k,
				r * this.shape.ay * view.k,
				0,
				0,
				Math.PI * 2,
			);
			ctx.stroke();
		}
		ctx.restore();

		const types = new Map<string, GraphNodeType>(
			this.nodes.map((n) => [n.id, n.data.type]),
		);
		this.drawEdges(types);
		this.drawNodes();
		this.drawLabels();
	}

	private edgeGeometry(l: SimLink) {
		const s = l.source as SimNode;
		const t = l.target as SimNode;
		const ax = this.sx(s.x ?? 0);
		const ay = this.sy(s.y ?? 0);
		const bx = this.sx(t.x ?? 0);
		const by = this.sy(t.y ?? 0);
		const c = curveControl(ax, ay, bx, by, l.bend);
		return { s, t, ax, ay, bx, by, cx: c.x, cy: c.y };
	}

	private drawEdges(types: Map<string, GraphNodeType>) {
		const { ctx, theme } = this;
		const batches = new Map<
			string,
			{
				color: string;
				width: number;
				alpha: number;
				dash: boolean;
				paths: number[][];
			}
		>();
		const glowPaths: { color: string; geo: number[] }[] = [];
		const dots: { x: number; y: number; color: string; alpha: number }[] = [];
		for (const l of this.links) {
			if (typeof l.source === 'string' || typeof l.target === 'string')
				continue;
			const g = this.edgeGeometry(l);
			const typeKey = edgeColorType(l.edge, types, this.center);
			const color = theme.colors[typeKey];
			const touchesCenter =
				l.edge.from === this.center || l.edge.to === this.center;
			const base = touchesCenter ? 0.62 : 0.28;
			const fade = Math.min(g.s.a, g.t.a, 1) * (l.ea > 1 ? 1 : 1);
			const emphasised = l.ea > 1;
			const alpha = emphasised
				? Math.min(1, 0.55 + (l.ea - 1) * 0.45) * fade
				: Math.min(base * l.ea, 1) * fade;
			if (alpha < 0.02) continue;
			const w =
				Math.min(3, 0.9 + l.edge.weight * 0.55) + (emphasised ? 0.8 : 0);
			const dash = l.edge.weight <= 1;
			const key = `${color}|${Math.round(alpha * 14)}|${w.toFixed(1)}|${
				dash ? 1 : 0
			}`;
			let batch = batches.get(key);
			if (!batch) {
				batch = {
					color,
					width: w,
					alpha: Math.round(alpha * 14) / 14,
					dash,
					paths: [],
				};
				batches.set(key, batch);
			}
			batch.paths.push([g.ax, g.ay, g.cx, g.cy, g.bx, g.by]);
			if (emphasised)
				glowPaths.push({ color, geo: [g.ax, g.ay, g.cx, g.cy, g.bx, g.by] });
			if (touchesCenter && alpha > 0.25 && this.links.length < 120) {
				const p = quadPoint(
					g.ax,
					g.ay,
					g.cx,
					g.cy,
					g.bx,
					g.by,
					0.28 + (l.pulse % 0.4),
				);
				dots.push({ x: p.x, y: p.y, color, alpha: alpha });
			}
		}
		ctx.lineCap = 'round';
		if (glowPaths.length) {
			ctx.save();
			ctx.globalAlpha = theme.dark ? 0.22 : 0.16;
			ctx.lineWidth = 7;
			for (const gp of glowPaths) {
				ctx.strokeStyle = gp.color;
				ctx.beginPath();
				ctx.moveTo(gp.geo[0], gp.geo[1]);
				ctx.quadraticCurveTo(gp.geo[2], gp.geo[3], gp.geo[4], gp.geo[5]);
				ctx.stroke();
			}
			ctx.restore();
		}
		for (const batch of batches.values()) {
			ctx.globalAlpha = batch.alpha;
			ctx.strokeStyle = batch.color;
			ctx.lineWidth = batch.width;
			ctx.setLineDash(batch.dash ? [5, 5] : []);
			ctx.beginPath();
			for (const p of batch.paths) {
				ctx.moveTo(p[0], p[1]);
				ctx.quadraticCurveTo(p[2], p[3], p[4], p[5]);
			}
			ctx.stroke();
		}
		ctx.setLineDash([]);
		for (const d of dots) {
			ctx.globalAlpha = Math.min(1, d.alpha + 0.2);
			ctx.fillStyle = d.color;
			ctx.beginPath();
			ctx.arc(d.x, d.y, 2.2, 0, Math.PI * 2);
			ctx.fill();
		}
		ctx.globalAlpha = 1;
	}

	private shapePath(shape: GraphShape, x: number, y: number, r: number) {
		const { ctx } = this;
		ctx.beginPath();
		if (shape === 'square') {
			const s = r * 0.9;
			ctx.roundRect(x - s, y - s, s * 2, s * 2, r * 0.3);
		} else if (shape === 'diamond') {
			const s = r * 1.2;
			ctx.moveTo(x, y - s);
			ctx.lineTo(x + s, y);
			ctx.lineTo(x, y + s);
			ctx.lineTo(x - s, y);
			ctx.closePath();
		} else {
			ctx.arc(x, y, shape === 'ring' ? r * 0.8 : r, 0, Math.PI * 2);
		}
	}

	private drawNodes() {
		const { ctx, theme, view } = this;
		const glow = this.nodes.length <= GLOW_NODE_LIMIT;
		const ordered = [...this.nodes].sort((a, b) => b.data.hop - a.data.hop);
		for (const n of ordered) {
			const alpha = n.a * n.dim;
			if (alpha < 0.02) continue;
			const x = this.sx(n.x ?? 0);
			const y = this.sy(n.y ?? 0);
			if (x < -80 || y < -80 || x > this.width + 80 || y > this.height + 80)
				continue;
			const r = n.r * view.k;
			const style = TYPE_STYLES[n.data.type];
			const color = theme.colors[n.data.type];
			const isCenter = n.data.hop === 0;
			const emphasised =
				n.id === this.hoverId ||
				n.id === this.selectedId ||
				(this.hood && n.dim > 0.9);
			if (
				glow &&
				(n.data.hop <= 1 || emphasised) &&
				n.data.type !== 'comment'
			) {
				const size = r * (isCenter ? 4.4 : 3.2);
				ctx.globalAlpha =
					alpha *
					(isCenter ? 0.85 : emphasised ? 0.7 : 0.5) *
					(theme.dark ? 1 : 0.7);
				ctx.drawImage(
					this.sprite(color),
					x - size / 2,
					y - size / 2,
					size,
					size,
				);
			}
			ctx.globalAlpha = alpha;
			ctx.fillStyle = color;
			this.shapePath(style.shape, x, y, r);
			ctx.fill();
			if (style.shape === 'ring') {
				ctx.strokeStyle = color;
				ctx.lineWidth = 1.6;
				ctx.beginPath();
				ctx.arc(x, y, r * 1.18, 0, Math.PI * 2);
				ctx.stroke();
			}
			if (isCenter) {
				ctx.strokeStyle = theme.centerRing;
				ctx.lineWidth = 2;
				ctx.beginPath();
				ctx.arc(x, y, r, 0, Math.PI * 2);
				ctx.stroke();
			}
			if (n.id === this.selectedId && !isCenter) {
				ctx.strokeStyle = theme.ink;
				ctx.lineWidth = 1.5;
				ctx.setLineDash([4, 4]);
				ctx.beginPath();
				ctx.arc(x, y, r * 1.35 + 3, 0, Math.PI * 2);
				ctx.stroke();
				ctx.setLineDash([]);
			}
		}
		ctx.globalAlpha = 1;
	}

	private nodeLines(n: SimNode): { title: string; sub: string | null } {
		const d = n.data;
		const type = TYPE_STYLES[d.type].label.toLowerCase();
		if (n.data.hop === 0) {
			return {
				title: truncateLabel(d.key || d.title, 22),
				sub: d.key ? 'this task' : 'this page',
			};
		}
		if (d.key) return { title: d.key, sub: truncateLabel(d.title, 26) };
		const extra =
			d.type === 'agent_run'
				? d.status?.name
				: d.type === 'page'
				? (d.meta?.page_type as string | undefined)
				: null;
		return {
			title: truncateLabel(d.title, 22),
			sub: extra && extra !== 'plain' ? `${type} · ${extra}` : type,
		};
	}

	private drawLabels() {
		const { ctx, theme, view } = this;
		const placed: LabelRect[] = [];
		const obstacles = this.nodes
			.filter((n) => n.a * n.dim > 0.12)
			.map((n) => {
				const r = n.r * view.k + 2;
				return {
					id: n.id,
					rect: {
						x: this.sx(n.x ?? 0) - r,
						y: this.sy(n.y ?? 0) - r,
						w: r * 2,
						h: r * 2,
					},
				};
			});
		const family = theme.fontFamily;
		const order = [...this.nodes]
			.filter((n) => n.a * n.dim > 0.12)
			.sort((a, b) => this.labelPriority(b) - this.labelPriority(a));
		ctx.textAlign = 'center';
		ctx.textBaseline = 'alphabetic';
		for (const n of order) {
			const hot =
				n.id === this.hoverId ||
				n.id === this.selectedId ||
				!!(this.hood && n.dim > 0.9);
			const hop = n.data.hop;
			if (hop === 2 && !hot && view.k < 0.5) continue;
			if (hop === 1 && !hot && view.k < 0.25) continue;
			if (n.data.type === 'comment' && !hot) continue;
			const x = this.sx(n.x ?? 0);
			const y = this.sy(n.y ?? 0);
			if (x < -40 || y < -40 || x > this.width + 40 || y > this.height + 40)
				continue;
			const r = n.r * view.k;
			const lines = this.nodeLines(n);
			const small = hop === 2 && !hot;
			const fs = Math.min(
				1,
				Math.max(0.82, Math.min(this.width, this.height) / 520),
			);
			const titleSize = Math.round((hop === 0 ? 14 : small ? 11 : 13) * fs);
			const subSize = Math.round(11 * fs);
			const showSub =
				!small && lines.sub && (hop === 0 || !this.options.compact);
			ctx.font = `${small ? 400 : 600} ${titleSize}px ${family}`;
			let w = ctx.measureText(
				small ? truncateLabel(lines.title, 16) : lines.title,
			).width;
			if (showSub) {
				ctx.font = `400 ${subSize}px ${family}`;
				w = Math.max(w, ctx.measureText(lines.sub!).width);
			}
			const h = small
				? titleSize + 2
				: titleSize + (showSub ? subSize + 3 : 0) + 2;
			const pad = 5;
			const gap = r + 5;
			const candidates: [number, number][] = [
				[x - w / 2 - pad, y + gap],
				[x - w / 2 - pad, y - gap - h - pad],
				[x + gap, y - h / 2],
				[x - gap - w - pad * 2, y - h / 2],
				[x + gap * 0.6, y + gap * 0.6],
				[x - gap * 0.6 - w - pad * 2, y - gap * 0.6 - h],
			];
			let chosen: LabelRect | null = null;
			for (const [rx, ry] of candidates) {
				const rect = { x: rx, y: ry, w: w + pad * 2, h: h + pad };
				if (
					!overlaps(rect, placed) &&
					!obstacles.some((o) => o.id !== n.id && overlaps(rect, [o.rect]))
				) {
					chosen = rect;
					break;
				}
			}
			if (!chosen) {
				if (hop !== 0 && !hot) continue;
				const [rx, ry] = candidates[0];
				chosen = { x: rx, y: ry, w: w + pad * 2, h: h + pad };
			}
			placed.push(chosen);
			const alpha = Math.min(1, n.a) * (n.dim < 0.5 ? n.dim * 1.2 : 1);
			ctx.globalAlpha = alpha * (small ? 0.85 : 1);
			if (!small) {
				ctx.fillStyle = theme.pill;
				ctx.beginPath();
				ctx.roundRect(chosen.x, chosen.y, chosen.w, chosen.h, 6);
				ctx.fill();
			}
			const tx = chosen.x + chosen.w / 2;
			let ty = chosen.y + pad / 2 + titleSize;
			ctx.fillStyle = small ? theme.inkMuted : theme.ink;
			ctx.font = `${small ? 400 : 600} ${titleSize}px ${family}`;
			ctx.fillText(
				small ? truncateLabel(lines.title, 16) : lines.title,
				tx,
				ty,
			);
			if (showSub) {
				ty += subSize + 3;
				ctx.fillStyle = theme.inkMuted;
				ctx.font = `400 ${subSize}px ${family}`;
				ctx.fillText(lines.sub!, tx, ty);
			}
		}
		ctx.globalAlpha = 1;
		this.drawEdgeLabels(placed);
	}

	private labelPriority(n: SimNode): number {
		if (n.data.hop === 0) return 100;
		if (n.id === this.hoverId) return 90;
		if (n.id === this.selectedId) return 80;
		return 50 - n.data.hop * 10 + n.data.weight;
	}

	private drawEdgeLabels(placed: LabelRect[]) {
		const { ctx, theme, view } = this;
		if (view.k < 0.45) return;
		ctx.font = `400 11px ${theme.fontFamily}`;
		ctx.textAlign = 'center';
		ctx.textBaseline = 'middle';
		const seen: LabelRect[] = [];
		for (const l of this.links) {
			if (typeof l.source === 'string' || typeof l.target === 'string')
				continue;
			const touchesCenter =
				l.edge.from === this.center || l.edge.to === this.center;
			const hot = l.ea > 1;
			if (!touchesCenter && !hot) continue;
			if (l.ea < 0.5) continue;
			const g = this.edgeGeometry(l);
			const p = quadPoint(g.ax, g.ay, g.cx, g.cy, g.bx, g.by, 0.5);
			const text = l.edge.label;
			const w = ctx.measureText(text).width;
			const rect = { x: p.x - w / 2 - 3, y: p.y - 8, w: w + 6, h: 16 };
			if (overlaps(rect, placed) || overlaps(rect, seen)) continue;
			seen.push(rect);
			let angle = Math.atan2(g.by - g.ay, g.bx - g.ax);
			if (angle > Math.PI / 2) angle -= Math.PI;
			if (angle < -Math.PI / 2) angle += Math.PI;
			ctx.save();
			ctx.translate(p.x, p.y);
			ctx.rotate(angle);
			ctx.globalAlpha = Math.min(1, Math.min(g.s.a, g.t.a)) * (hot ? 1 : 0.9);
			ctx.lineWidth = 4;
			ctx.lineJoin = 'round';
			ctx.strokeStyle = theme.bg;
			ctx.strokeText(text, 0, 0);
			ctx.fillStyle = hot ? theme.ink : theme.inkMuted;
			ctx.fillText(text, 0, 0);
			ctx.restore();
		}
		ctx.globalAlpha = 1;
	}

	private local(e: PointerEvent | WheelEvent | MouseEvent) {
		const rect = this.canvas.getBoundingClientRect();
		return { x: e.clientX - rect.left, y: e.clientY - rect.top };
	}

	private world(px: number, py: number) {
		return {
			x: (px - this.view.x) / this.view.k,
			y: (py - this.view.y) / this.view.k,
		};
	}

	private hit(px: number, py: number): SimNode | null {
		let best: SimNode | null = null;
		let bestDist = Infinity;
		for (const n of this.nodes) {
			if (n.leaving || n.a < 0.3) continue;
			const r = Math.max(n.r * this.view.k * 1.25, 11);
			const dx = this.sx(n.x ?? 0) - px;
			const dy = this.sy(n.y ?? 0) - py;
			const d = Math.hypot(dx, dy);
			if (d <= r && d < bestDist) {
				best = n;
				bestDist = d;
			}
		}
		return best;
	}

	private setHover(node: SimNode | null) {
		const id = node?.id ?? null;
		if (id === this.hoverId) return;
		this.hoverId = id;
		this.hood = id
			? neighbourhood(
					this.links.map((l) => l.edge),
					id,
			  )
			: null;
		this.canvas.style.cursor = id ? 'pointer' : this.down ? 'grabbing' : 'grab';
		this.applyEmphasis();
		this.handlers.onHover(id);
		this.wake();
	}

	private onPointerDown = (e: PointerEvent) => {
		try {
			this.canvas.setPointerCapture(e.pointerId);
		} catch {
			// synthetic pointers cannot be captured
		}
		const p = this.local(e);
		this.pointers.set(e.pointerId, p);
		if (this.pointers.size === 2) {
			const [a, b] = [...this.pointers.values()];
			this.finishDrag();
			this.pinch = { dist: Math.hypot(a.x - b.x, a.y - b.y) };
			return;
		}
		this.down = { x: p.x, y: p.y, node: this.hit(p.x, p.y), moved: false };
	};

	private onPointerMove = (e: PointerEvent) => {
		const p = this.local(e);
		if (this.pointers.has(e.pointerId)) this.pointers.set(e.pointerId, p);
		if (this.pinch && this.pointers.size === 2) {
			const [a, b] = [...this.pointers.values()];
			const dist = Math.hypot(a.x - b.x, a.y - b.y);
			if (this.pinch.dist > 0) {
				this.zoomAt((a.x + b.x) / 2, (a.y + b.y) / 2, dist / this.pinch.dist);
			}
			this.pinch.dist = dist;
			return;
		}
		const down = this.down;
		if (!down) {
			this.setHover(this.hit(p.x, p.y));
			return;
		}
		if (!down.moved && Math.hypot(p.x - down.x, p.y - down.y) > 4) {
			down.moved = true;
			if (down.node) {
				this.sim.alphaTarget(0.22);
				this.setHover(down.node);
			} else {
				this.autoFit = false;
				this.canvas.style.cursor = 'grabbing';
			}
		}
		if (!down.moved) return;
		if (down.node) {
			const w = this.world(p.x, p.y);
			down.node.fx = w.x;
			down.node.fy = w.y;
		} else {
			this.view.x += p.x - down.x;
			this.view.y += p.y - down.y;
			down.x = p.x;
			down.y = p.y;
		}
		this.wake();
	};

	private onPointerUp = (e: PointerEvent) => {
		this.pointers.delete(e.pointerId);
		if (this.pinch) {
			if (this.pointers.size < 2) this.pinch = null;
			return;
		}
		const down = this.down;
		if (!down) return;
		if (!down.moved) {
			this.down = null;
			this.handlers.onSelect(down.node?.id ?? null);
		} else this.finishDrag();
		this.canvas.style.cursor = this.hoverId ? 'pointer' : 'grab';
		this.wake();
	};

	private finishDrag() {
		const down = this.down;
		this.down = null;
		if (!down?.moved || !down.node) return;
		const pin = down.node.data.hop === 0 ? 0 : null;
		down.node.fx = pin;
		down.node.fy = pin;
		this.sim.alphaTarget(0);
		this.wake();
	}

	private onPointerLeave = () => {
		if (!this.down) this.setHover(null);
	};

	private onDblClick = (e: MouseEvent) => {
		const p = this.local(e);
		const node = this.hit(p.x, p.y);
		if (node && node.data.hop !== 0 && isExpandable(node.data.type))
			this.handlers.onCenter(node.id);
	};

	private onWheel = (e: WheelEvent) => {
		if (this.options.wheelZoom === 'modifier' && !(e.ctrlKey || e.metaKey))
			return;
		e.preventDefault();
		const p = this.local(e);
		this.zoomAt(p.x, p.y, Math.exp(-e.deltaY * (e.ctrlKey ? 0.01 : 0.0016)));
	};

	private zoomAt(px: number, py: number, factor: number) {
		const k = Math.min(MAX_K, Math.max(MIN_K, this.view.k * factor));
		const ratio = k / this.view.k;
		this.view = {
			k,
			x: px - (px - this.view.x) * ratio,
			y: py - (py - this.view.y) * ratio,
		};
		this.autoFit = false;
		this.wake();
	}
}

const withAlpha = (hex: string, alpha: number): string => {
	const m = /^#([0-9a-f]{6})$/i.exec(hex);
	if (!m) return hex;
	const n = parseInt(m[1], 16);
	return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${alpha})`;
};
