import {
	curveControl,
	fitTransform,
	hashString,
	readThemeFromDom,
	type GraphTheme,
	type Transform,
} from '../graphLogic';
import { createMapLayout, type MapLayout } from './mapLayoutClient';
import type { LayoutStep } from './mapLayoutCore';
import {
	BOTTLENECK_COLOR,
	buildLayoutInit,
	labelLevel,
	labelTone,
	mixHex,
	rgba,
	shouldLabel,
	tone,
	visibleAt,
	type PreparedMap,
} from './mapLogic';

export interface MapEngineHandlers {
	onSelect: (id: string) => void;
	onHover: (index: number | null) => void;
	onLayout: (info: { ms: number; ticks: number; running: boolean }) => void;
}

interface ClusterGeo {
	x: number;
	y: number;
	r: number;
	top: number;
	visible: number;
}

interface BridgeGroup {
	a: number;
	b: number;
	edges: Int32Array;
	sign: number;
	top: boolean;
}

interface Rect {
	x: number;
	y: number;
	w: number;
	h: number;
}

const MIN_K = 0.04;
const MAX_K = 7;
const TAU = Math.PI * 2;

const hit = (a: Rect, list: Rect[]): boolean => {
	for (const b of list) {
		if (
			a.x < b.x + b.w &&
			a.x + a.w > b.x &&
			a.y < b.y + b.h &&
			a.y + a.h > b.y
		)
			return true;
	}
	return false;
};

const ease = (t: number) =>
	t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2;

export class MapEngine {
	private ctx: CanvasRenderingContext2D;
	private theme: GraphTheme;
	private layout: MapLayout;
	private data: PreparedMap | null = null;
	private count = 0;
	private cur = new Float32Array(0);
	private tgt = new Float32Array(0);
	private sx = new Float32Array(0);
	private sy = new Float32Array(0);
	private alpha = new Float32Array(0);
	private want = new Float32Array(0);
	private geo: ClusterGeo[] = [];
	private intra: Int32Array[] = [];
	private members: Int32Array[] = [];
	private bridges: BridgeGroup[] = [];
	private view: Transform = { k: 1, x: 0, y: 0 };
	private fitK = 1;
	private width = 0;
	private height = 0;
	private dpr = 1;
	private raf = 0;
	private dirty = true;
	private hasPos = false;
	private layoutRunning = false;
	private layoutStart = 0;
	private autoFit = true;
	private cursor = Infinity;
	private showOrphans = true;
	private group = true;
	private matches: number[] | null = null;
	private matchSet: Uint8Array | null = null;
	private pinned: Uint8Array | null = null;
	private hover = -1;
	private focusNode: Uint8Array | null = null;
	private focusEdge: Uint8Array | null = null;
	private anim: {
		from: Transform;
		to: Transform;
		t0: number;
		ms: number;
	} | null = null;
	private sprites = new Map<string, HTMLCanvasElement>();
	private observer: ResizeObserver | null = null;
	private pointers = new Map<number, { x: number; y: number }>();
	private press: { x: number; y: number; moved: boolean } | null = null;
	private pinch: number | null = null;
	private destroyed = false;
	private reduced: boolean;

	constructor(
		private canvas: HTMLCanvasElement,
		private handlers: MapEngineHandlers,
	) {
		this.ctx = canvas.getContext('2d')!;
		this.theme = readThemeFromDom(canvas);
		this.reduced =
			typeof window.matchMedia === 'function' &&
			window.matchMedia('(prefers-reduced-motion: reduce)').matches;
		this.layout = createMapLayout(this.onStep);
		canvas.addEventListener('pointerdown', this.onDown);
		canvas.addEventListener('pointermove', this.onMove);
		canvas.addEventListener('pointerup', this.onUp);
		canvas.addEventListener('pointercancel', this.onUp);
		canvas.addEventListener('pointerleave', this.onLeave);
		canvas.addEventListener('dblclick', this.onDbl);
		canvas.addEventListener('wheel', this.onWheel, { passive: false });
		this.observer = new ResizeObserver(() => this.resize());
		this.observer.observe(canvas);
		this.resize();
	}

	setData(prepared: PreparedMap, group: boolean, showOrphans: boolean) {
		this.data = prepared;
		this.count = prepared.nodes.length;
		this.group = group;
		this.showOrphans = showOrphans;
		this.cur = new Float32Array(this.count * 2);
		this.tgt = new Float32Array(this.count * 2);
		this.sx = new Float32Array(this.count);
		this.sy = new Float32Array(this.count);
		this.alpha = new Float32Array(this.count);
		this.want = new Float32Array(this.count);
		this.hasPos = false;
		this.hover = -1;
		this.matches = null;
		this.matchSet = null;
		this.pinned = null;
		this.autoFit = true;
		this.anim = null;

		const perCluster: number[][] = prepared.clusters.map(() => []);
		const pairs = new Map<string, number[]>();
		for (let e = 0; e < prepared.edges.length; e++) {
			const a = prepared.cluster[prepared.edgeS[e]];
			const b = prepared.cluster[prepared.edgeT[e]];
			if (a === b) perCluster[a].push(e);
			else {
				const key = a < b ? `${a}:${b}` : `${b}:${a}`;
				const list = pairs.get(key);
				if (list) list.push(e);
				else pairs.set(key, [e]);
			}
		}
		this.intra = perCluster.map((l) => Int32Array.from(l));
		const byCluster: number[][] = prepared.clusters.map(() => []);
		for (let i = 0; i < this.count; i++) byCluster[prepared.cluster[i]].push(i);
		this.members = byCluster.map((l) => Int32Array.from(l));
		this.bridges = [...pairs.entries()]
			.map(([key, list]) => {
				const [a, b] = key.split(':').map(Number);
				return {
					a,
					b,
					edges: Int32Array.from(list),
					sign: hashString(key) & 1 ? 1 : -1,
					top: false,
				};
			})
			.sort((x, y) => y.edges.length - x.edges.length);
		this.bridges.slice(0, 10).forEach((g) => (g.top = true));
		this.geo = prepared.clusters.map(() => ({
			x: 0,
			y: 0,
			r: 0,
			top: 0,
			visible: 0,
		}));
		this.applyWanted();
		this.layoutStart = performance.now();
		this.layoutRunning = true;
		this.layout.start(buildLayoutInit(prepared, group));
		this.wake();
	}

	setCursor(ms: number) {
		this.cursor = ms;
		this.applyWanted();
		this.wake();
	}

	setShowOrphans(show: boolean) {
		this.showOrphans = show;
		this.applyWanted();
		this.wake();
	}

	setGroup(group: boolean) {
		if (group === this.group) return;
		this.group = group;
		this.layoutStart = performance.now();
		this.layoutRunning = true;
		this.layout.setGroup(group);
		this.wake();
	}

	setSearch(matches: number[] | null) {
		this.matches = matches;
		if (matches && this.count) {
			this.matchSet = new Uint8Array(this.count);
			matches.forEach((i) => (this.matchSet![i] = 1));
		} else this.matchSet = null;
		this.rebuildFocus();
	}

	pin(indices: number[] | null) {
		if (!indices || !indices.length || !this.count) this.pinned = null;
		else {
			this.pinned = new Uint8Array(this.count);
			indices.forEach((i) => (this.pinned![i] = 1));
		}
		this.rebuildFocus();
	}

	focusIndices(indices: number[], maxK = 2.2) {
		if (!indices.length || !this.hasPos) return;
		const target = this.boundsTransform(
			indices.map((i) => [this.tgt[i * 2], this.tgt[i * 2 + 1]]),
			110,
			maxK,
		);
		this.flyTo(target);
	}

	focusCluster(c: number) {
		const g = this.geo[c];
		if (!g || !g.visible) return;
		const { k, x, y } = this.view;
		const wx = (g.x - x) / k;
		const wy = (g.y - y) / k;
		const wr = g.r / k;
		this.flyTo(
			this.boundsTransform(
				[
					[wx - wr, wy - wr],
					[wx + wr, wy + wr],
				],
				36,
				3.2,
			),
		);
	}

	fit() {
		this.autoFit = false;
		this.flyTo(this.fitTarget());
	}

	zoomBy(factor: number) {
		this.autoFit = false;
		this.zoomAt(this.width / 2, this.height / 2, factor);
	}

	refreshTheme() {
		this.theme = readThemeFromDom(this.canvas);
		this.sprites.clear();
		this.wake();
	}

	destroy() {
		this.destroyed = true;
		cancelAnimationFrame(this.raf);
		this.layout.dispose();
		this.observer?.disconnect();
		const c = this.canvas;
		c.removeEventListener('pointerdown', this.onDown);
		c.removeEventListener('pointermove', this.onMove);
		c.removeEventListener('pointerup', this.onUp);
		c.removeEventListener('pointercancel', this.onUp);
		c.removeEventListener('pointerleave', this.onLeave);
		c.removeEventListener('dblclick', this.onDbl);
		c.removeEventListener('wheel', this.onWheel);
	}

	private applyWanted() {
		const d = this.data;
		if (!d) return;
		for (let i = 0; i < this.count; i++) {
			const on =
				visibleAt(d.createdMs[i], this.cursor) &&
				(!d.orphan[i] || this.showOrphans);
			this.want[i] = on ? 1 : 0;
			if (this.reduced || !this.hasPos) this.alpha[i] = this.want[i];
		}
	}

	private onStep = (s: LayoutStep) => {
		if (this.destroyed || s.positions.length !== this.tgt.length) return;
		this.tgt = s.positions;
		this.layoutRunning = !s.done;
		if (!this.hasPos) {
			this.cur.set(s.positions);
			this.hasPos = true;
			this.applyWanted();
			this.view = this.fitTarget();
		}
		if (s.done) {
			this.handlers.onLayout({
				ms: performance.now() - this.layoutStart,
				ticks: s.ticks,
				running: false,
			});
		}
		this.wake();
	};

	private wake() {
		this.dirty = true;
		if (this.raf || this.destroyed) return;
		this.raf = requestAnimationFrame(this.frame);
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
		if (this.hasPos && this.autoFit) this.view = this.fitTarget();
		this.wake();
	}

	private boundsTransform(
		points: number[][],
		padding: number,
		maxK: number,
	): Transform {
		let minX = Infinity;
		let minY = Infinity;
		let maxX = -Infinity;
		let maxY = -Infinity;
		for (const [x, y] of points) {
			if (x < minX) minX = x;
			if (x > maxX) maxX = x;
			if (y < minY) minY = y;
			if (y > maxY) maxY = y;
		}
		if (!Number.isFinite(minX))
			return { k: 1, x: this.width / 2, y: this.height / 2 };
		const t = fitTransform(
			{ minX, minY, maxX, maxY },
			this.width,
			this.height,
			padding,
			maxK,
		);
		return { ...t, k: Math.min(MAX_K, Math.max(MIN_K, t.k)) };
	}

	private fitTarget(): Transform {
		const pts: number[][] = [];
		const d = this.data;
		if (d) {
			for (let i = 0; i < this.count; i++) {
				if (d.orphan[i] && !this.showOrphans) continue;
				pts.push([this.tgt[i * 2], this.tgt[i * 2 + 1]]);
			}
			if (pts.length) {
				let minX = Infinity;
				let minY = Infinity;
				let maxX = -Infinity;
				let maxY = -Infinity;
				for (const [x, y] of pts) {
					minX = Math.min(minX, x);
					maxX = Math.max(maxX, x);
					minY = Math.min(minY, y);
					maxY = Math.max(maxY, y);
				}
				const t = this.boundsTransform(
					[
						[minX, minY],
						[maxX, maxY],
					],
					Math.max(24, Math.min(this.width, this.height) * 0.08),
					1.4,
				);
				this.fitK = t.k;
				return t;
			}
		}
		return { k: 1, x: this.width / 2, y: this.height / 2 };
	}

	private flyTo(to: Transform) {
		this.autoFit = false;
		if (this.reduced) {
			this.view = to;
			this.anim = null;
		} else {
			this.anim = {
				from: { ...this.view },
				to,
				t0: performance.now(),
				ms: 650,
			};
		}
		this.wake();
	}

	private zoomAt(px: number, py: number, factor: number) {
		this.anim = null;
		const k = Math.min(MAX_K, Math.max(MIN_K, this.view.k * factor));
		const f = k / this.view.k;
		this.view = {
			k,
			x: px - (px - this.view.x) * f,
			y: py - (py - this.view.y) * f,
		};
		this.wake();
	}

	private rebuildFocus() {
		const d = this.data;
		if (!d) return;
		let nodes: Uint8Array | null = null;
		let edges: Uint8Array | null = null;
		if (this.hover >= 0) {
			nodes = new Uint8Array(this.count);
			edges = new Uint8Array(d.edges.length);
			nodes[this.hover] = 1;
			for (
				let a = d.adjStart[this.hover];
				a < d.adjStart[this.hover + 1];
				a++
			) {
				const e = d.adjEdge[a];
				edges[e] = 1;
				nodes[d.edgeS[e]] = 1;
				nodes[d.edgeT[e]] = 1;
			}
		} else {
			const base = this.matchSet ?? this.pinned;
			if (base) {
				nodes = base;
				edges = new Uint8Array(d.edges.length);
				for (let e = 0; e < d.edges.length; e++) {
					if (base[d.edgeS[e]] && base[d.edgeT[e]]) edges[e] = 1;
				}
			}
		}
		this.focusNode = nodes;
		this.focusEdge = edges;
		this.wake();
	}

	private sprite(color: string): HTMLCanvasElement {
		const key = `${color}${this.theme.dark}`;
		let s = this.sprites.get(key);
		if (s) return s;
		s = document.createElement('canvas');
		s.width = s.height = 96;
		const g = s.getContext('2d')!;
		const grad = g.createRadialGradient(48, 48, 0, 48, 48, 48);
		grad.addColorStop(0, rgba(color, this.theme.dark ? 0.9 : 0.7));
		grad.addColorStop(0.35, rgba(color, 0.35));
		grad.addColorStop(1, rgba(color, 0));
		g.fillStyle = grad;
		g.fillRect(0, 0, 96, 96);
		this.sprites.set(key, s);
		return s;
	}

	private frame = (now: number) => {
		this.raf = 0;
		if (this.destroyed || !this.data) return;
		let again = this.layoutRunning;

		if (this.anim) {
			const t = Math.min(1, (now - this.anim.t0) / this.anim.ms);
			const e = ease(t);
			const { from, to } = this.anim;
			this.view = {
				k: from.k * Math.pow(to.k / from.k, e),
				x: from.x + (to.x - from.x) * e,
				y: from.y + (to.y - from.y) * e,
			};
			if (t >= 1) this.anim = null;
			else again = true;
		} else if (this.autoFit && this.hasPos && this.layoutRunning) {
			const t = this.fitTarget();
			this.view = {
				k: this.view.k + (t.k - this.view.k) * 0.2,
				x: this.view.x + (t.x - this.view.x) * 0.2,
				y: this.view.y + (t.y - this.view.y) * 0.2,
			};
		} else if (this.autoFit && this.hasPos && this.dirty) {
			this.view = this.fitTarget();
		}

		let moving = false;
		const cur = this.cur;
		const tgt = this.tgt;
		for (let i = 0; i < cur.length; i++) {
			const d = tgt[i] - cur[i];
			if (d > 0.04 || d < -0.04) {
				cur[i] += d * 0.3;
				moving = true;
			} else cur[i] = tgt[i];
		}
		let fading = false;
		for (let i = 0; i < this.count; i++) {
			const d = this.want[i] - this.alpha[i];
			if (d > 0.01 || d < -0.01) {
				this.alpha[i] += d * 0.2;
				fading = true;
			} else this.alpha[i] = this.want[i];
		}
		if (moving || fading) again = true;

		this.dirty = false;
		this.draw();
		if (again) this.raf = requestAnimationFrame(this.frame);
	};

	private project() {
		const { k, x, y } = this.view;
		const cur = this.cur;
		for (let i = 0; i < this.count; i++) {
			this.sx[i] = cur[i * 2] * k + x;
			this.sy[i] = cur[i * 2 + 1] * k + y;
		}
	}

	private computeGeo() {
		const d = this.data!;
		const acc = d.clusters.map(() => ({ x: 0, y: 0, n: 0, top: Infinity }));
		for (let i = 0; i < this.count; i++) {
			if (d.orphan[i] || this.alpha[i] < 0.5) continue;
			const a = acc[d.cluster[i]];
			a.x += this.sx[i];
			a.y += this.sy[i];
			a.n++;
			if (this.sy[i] < a.top) a.top = this.sy[i];
		}
		const far = d.clusters.map(() => 0);
		for (let i = 0; i < this.count; i++) {
			if (d.orphan[i] || this.alpha[i] < 0.5) continue;
			const c = d.cluster[i];
			const a = acc[c];
			const dist = Math.hypot(this.sx[i] - a.x / a.n, this.sy[i] - a.y / a.n);
			if (dist > far[c]) far[c] = dist;
		}
		acc.forEach((a, c) => {
			const g = this.geo[c];
			g.visible = a.n;
			if (!a.n) return;
			g.x = a.x / a.n;
			g.y = a.y / a.n;
			g.r = far[c] * 1.08 + 16;
			g.top = a.top - 12;
		});
	}

	private draw() {
		const ctx = this.ctx;
		const d = this.data!;
		const th = this.theme;
		const dark = th.dark;
		const W = this.width;
		const H = this.height;
		ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
		ctx.globalAlpha = 1;
		ctx.fillStyle = th.bgEdge;
		ctx.fillRect(0, 0, W, H);
		if (!this.hasPos || !this.count) return;

		this.project();
		this.computeGeo();
		const { k } = this.view;
		const sx = this.sx;
		const sy = this.sy;
		const al = this.alpha;
		const fn = this.focusNode;
		const fe = this.focusEdge;
		const dimmed = fn !== null;
		const margin = 40;
		const inView = (i: number) =>
			sx[i] > -margin &&
			sx[i] < W + margin &&
			sy[i] > -margin &&
			sy[i] < H + margin;
		const scale = Math.pow(k, 0.5) * 1.35;
		const rpx = (i: number) => Math.max(2, d.radius[i] * scale);
		const colors = d.clusters.map((c) => tone(c.color, dark));

		d.clusters.forEach((c, ci) => {
			const g = this.geo[ci];
			if (!g.visible) return;
			const hz = g.r * 1.3;
			const grad = ctx.createRadialGradient(g.x, g.y, 0, g.x, g.y, hz);
			const strength = (dark ? 0.17 : 0.2) * (dimmed ? 0.5 : 1);
			const f = 0.35 + 0.65 * (g.visible / Math.max(1, c.count));
			grad.addColorStop(0, rgba(colors[ci], strength * f));
			grad.addColorStop(0.6, rgba(colors[ci], strength * f * 0.45));
			grad.addColorStop(1, rgba(colors[ci], 0));
			ctx.fillStyle = grad;
			ctx.fillRect(g.x - hz, g.y - hz, hz * 2, hz * 2);
		});

		const thin = d.edges.length > 2500 && this.view.k < this.fitK * 1.6;
		const edgeBase = (dark ? 0.2 : 0.3) * (thin ? 0.7 : 1);
		ctx.lineWidth = 1;
		ctx.lineCap = 'round';
		const fading: number[] = [];
		for (let ci = 0; ci < this.intra.length; ci++) {
			const list = this.intra[ci];
			for (let pass = 0; pass < (dimmed ? 2 : 1); pass++) {
				ctx.beginPath();
				let any = false;
				for (let n = 0; n < list.length; n++) {
					const e = list[n];
					const s = d.edgeS[e];
					const t = d.edgeT[e];
					const a = Math.min(al[s], al[t]);
					if (a === 0) continue;
					if (dimmed && (fe![e] === 1) !== (pass === 1)) continue;
					if (!inView(s) && !inView(t)) {
						if (
							(sx[s] < 0 && sx[t] < 0) ||
							(sx[s] > W && sx[t] > W) ||
							(sy[s] < 0 && sy[t] < 0) ||
							(sy[s] > H && sy[t] > H)
						)
							continue;
					}
					if (a < 0.99) {
						if (pass === 0) fading.push(e);
						continue;
					}
					ctx.moveTo(sx[s], sy[s]);
					ctx.lineTo(sx[t], sy[t]);
					any = true;
				}
				if (!any) continue;
				ctx.strokeStyle = rgba(
					colors[ci],
					dimmed ? (pass === 1 ? edgeBase * 2.2 : edgeBase * 0.3) : edgeBase,
				);
				ctx.stroke();
			}
		}
		for (const e of fading) {
			const s = d.edgeS[e];
			const t = d.edgeT[e];
			ctx.globalAlpha = Math.min(al[s], al[t]);
			ctx.strokeStyle = rgba(colors[d.cluster[s]], edgeBase);
			ctx.beginPath();
			ctx.moveTo(sx[s], sy[s]);
			ctx.lineTo(sx[t], sy[t]);
			ctx.stroke();
		}
		ctx.globalAlpha = 1;

		for (const g of this.bridges) {
			const ga = this.geo[g.a];
			const gb = this.geo[g.b];
			if (!ga.visible || !gb.visible) continue;
			const ca = mixHex(colors[g.a], dark ? '#ffffff' : '#000000', 0.25);
			const cb = mixHex(colors[g.b], dark ? '#ffffff' : '#000000', 0.25);
			const grad = ctx.createLinearGradient(ga.x, ga.y, gb.x, gb.y);
			grad.addColorStop(0, ca);
			grad.addColorStop(1, cb);
			for (let layer = g.top ? 0 : 1; layer < 2; layer++) {
				for (let pass = 0; pass < (dimmed ? 2 : 1); pass++) {
					ctx.beginPath();
					let any = false;
					for (let n = 0; n < g.edges.length; n++) {
						const e = g.edges[n];
						const s = d.edgeS[e];
						const t = d.edgeT[e];
						const a = Math.min(al[s], al[t]);
						if (a < 0.5) continue;
						if (dimmed && (fe![e] === 1) !== (pass === 1)) continue;
						if (
							(sx[s] < -margin && sx[t] < -margin) ||
							(sx[s] > W + margin && sx[t] > W + margin) ||
							(sy[s] < -margin && sy[t] < -margin) ||
							(sy[s] > H + margin && sy[t] > H + margin)
						)
							continue;
						const wobble = ((hashString(`${e}`) % 9) - 4) / 100;
						const c = curveControl(
							sx[s],
							sy[s],
							sx[t],
							sy[t],
							g.sign * 0.09 + wobble,
						);
						ctx.moveTo(sx[s], sy[s]);
						ctx.quadraticCurveTo(c.x, c.y, sx[t], sy[t]);
						any = true;
					}
					if (!any) continue;
					const strong = !dimmed || pass === 1;
					ctx.strokeStyle = grad;
					if (layer === 0) {
						ctx.lineWidth = 4;
						ctx.globalAlpha = strong ? 0.1 : 0.03;
					} else {
						ctx.lineWidth = 1.1;
						ctx.globalAlpha = strong ? (dark ? 0.5 : 0.55) : 0.07;
					}
					ctx.stroke();
				}
			}
		}
		ctx.globalAlpha = 1;

		if (d.bottleneck >= 0 && al[d.bottleneck] > 0) {
			ctx.strokeStyle = rgba(dark ? BOTTLENECK_COLOR : '#e8590c', 0.75);
			ctx.lineWidth = 1.5;
			ctx.beginPath();
			for (const e of d.bottleneckEdges) {
				const s = d.edgeS[e];
				const t = d.edgeT[e];
				if (al[s] < 0.5 || al[t] < 0.5) continue;
				ctx.moveTo(sx[s], sy[s]);
				ctx.lineTo(sx[t], sy[t]);
			}
			ctx.stroke();
		}

		if (this.showOrphans) {
			ctx.strokeStyle = dark ? '#6b7385' : '#8b93a5';
			ctx.lineWidth = 1.5;
			ctx.setLineDash([3, 3]);
			ctx.beginPath();
			for (let i = 0; i < this.count; i++) {
				if (!d.orphan[i] || al[i] < 0.5 || !inView(i)) continue;
				const r = Math.max(5, 6 * Math.pow(k, 0.35));
				ctx.moveTo(sx[i] + r, sy[i]);
				ctx.arc(sx[i], sy[i], r, 0, TAU);
			}
			ctx.stroke();
			ctx.setLineDash([]);
		}

		const emphasised = (i: number) => d.hub[i] === 1 || i === d.bottleneck;
		for (let ci = 0; ci < d.clusters.length; ci++) {
			for (let pass = 0; pass < (dimmed ? 2 : 1); pass++) {
				ctx.beginPath();
				let any = false;
				const list = this.members[ci];
				for (let n = 0; n < list.length; n++) {
					const i = list[n];
					if (d.orphan[i] || emphasised(i)) continue;
					if (al[i] < 0.99 || !inView(i)) continue;
					if (dimmed && (fn![i] === 1) !== (pass === 1)) continue;
					const r = rpx(i);
					ctx.moveTo(sx[i] + r, sy[i]);
					ctx.arc(sx[i], sy[i], r, 0, TAU);
					any = true;
				}
				if (!any) continue;
				ctx.fillStyle = rgba(
					colors[ci],
					dimmed ? (pass === 1 ? 0.95 : 0.16) : 0.85,
				);
				ctx.fill();
			}
		}
		for (let i = 0; i < this.count; i++) {
			if (al[i] <= 0 || al[i] >= 0.99 || d.orphan[i] || emphasised(i)) continue;
			ctx.globalAlpha = al[i] * 0.85;
			ctx.fillStyle = colors[d.cluster[i]];
			ctx.beginPath();
			ctx.arc(sx[i], sy[i], rpx(i), 0, TAU);
			ctx.fill();
		}
		ctx.globalAlpha = 1;

		for (let i = 0; i < this.count; i++) {
			if (!emphasised(i) || al[i] <= 0 || d.orphan[i] || !inView(i)) continue;
			const isB = i === d.bottleneck;
			const color = isB
				? dark
					? BOTTLENECK_COLOR
					: '#e8590c'
				: colors[d.cluster[i]];
			const out = dimmed && fn![i] !== 1;
			const r = rpx(i) * (isB ? 1.15 : 1);
			ctx.globalAlpha = al[i] * (out ? 0.3 : 1);
			const glow = r * (isB ? 4.4 : 3.6);
			ctx.drawImage(
				this.sprite(color),
				sx[i] - glow,
				sy[i] - glow,
				glow * 2,
				glow * 2,
			);
			ctx.fillStyle = color;
			ctx.beginPath();
			ctx.arc(sx[i], sy[i], r, 0, TAU);
			ctx.fill();
			if (isB) {
				ctx.strokeStyle = color;
				ctx.lineWidth = 1.5;
				ctx.beginPath();
				ctx.arc(sx[i], sy[i], r + 6, 0, TAU);
				ctx.stroke();
			}
		}
		ctx.globalAlpha = 1;

		if (this.matches) {
			ctx.strokeStyle = th.ink;
			ctx.lineWidth = 1.5;
			ctx.beginPath();
			let shown = 0;
			for (const i of this.matches) {
				if (al[i] < 0.5 || !inView(i)) continue;
				ctx.moveTo(sx[i] + rpx(i) + 3, sy[i]);
				ctx.arc(sx[i], sy[i], rpx(i) + 3, 0, TAU);
				if (++shown > 400) break;
			}
			ctx.stroke();
		}
		if (this.hover >= 0 && al[this.hover] > 0.5) {
			ctx.strokeStyle = th.ink;
			ctx.lineWidth = 2;
			ctx.beginPath();
			ctx.arc(sx[this.hover], sy[this.hover], rpx(this.hover) + 3.5, 0, TAU);
			ctx.stroke();
		}

		this.drawLabels();
		if (import.meta.env.DEV) this.publishDebug();
	}

	private publishDebug() {
		const d = this.data!;
		let visible = 0;
		let rings = 0;
		for (let i = 0; i < this.count; i++) {
			if (this.alpha[i] < 0.5) continue;
			if (d.orphan[i]) rings++;
			else visible++;
		}
		const spot = (i: number) => ({
			id: d.nodes[i].id,
			x: this.sx[i],
			y: this.sy[i],
		});
		this.canvas.dataset.debug = JSON.stringify({
			k: this.view.k,
			fitK: this.fitK,
			visible,
			rings,
			clusters: d.clusters.map((c, i) => ({
				title: c.title,
				x: this.geo[i].x,
				y: this.geo[i].y,
				r: this.geo[i].r,
				top: this.geo[i].top,
			})),
			hubs: d.clusters.filter((c) => c.hub >= 0).map((c) => spot(c.hub)),
			bottleneck: d.bottleneck >= 0 ? spot(d.bottleneck) : null,
		});
	}

	private drawLabels() {
		const ctx = this.ctx;
		const d = this.data!;
		const th = this.theme;
		const dark = th.dark;
		const { k } = this.view;
		const W = this.width;
		const H = this.height;
		const sx = this.sx;
		const sy = this.sy;
		const taken: Rect[] = [];
		const family = th.fontFamily;
		const halo = rgba(dark ? '#0b0d12' : '#ffffff', 0.85);
		ctx.textAlign = 'center';
		ctx.textBaseline = 'alphabetic';
		ctx.lineJoin = 'round';

		const put = (
			text: string,
			x: number,
			y: number,
			size: number,
			weight: number,
			color: string,
			mustShow: boolean,
			align: CanvasTextAlign = 'center',
		): boolean => {
			ctx.font = `${weight} ${size}px ${family}`;
			const w = ctx.measureText(text).width;
			const left =
				align === 'center' ? x - w / 2 : align === 'left' ? x : x - w;
			const rect = { x: left - 3, y: y - size, w: w + 6, h: size + 5 };
			if (
				rect.x > W ||
				rect.x + rect.w < 0 ||
				rect.y > H ||
				rect.y + rect.h < 0
			)
				return false;
			if (!mustShow && hit(rect, taken)) return false;
			taken.push(rect);
			ctx.textAlign = align;
			ctx.lineWidth = 4;
			ctx.strokeStyle = halo;
			ctx.strokeText(text, x, y);
			ctx.fillStyle = color;
			ctx.fillText(text, x, y);
			return true;
		};

		const size = Math.min(19, 14 + Math.max(0, Math.log2(k / this.fitK)) * 1.5);
		d.clusters.forEach((c, ci) => {
			const g = this.geo[ci];
			if (!g.visible) return;
			put(
				c.title,
				g.x,
				g.y - g.r * 0.96,
				size,
				600,
				labelTone(c.color, dark),
				true,
			);
		});

		const level = labelLevel(k / this.fitK);
		const order: number[] = [];
		if (d.bottleneck >= 0) order.push(d.bottleneck);
		d.clusters.forEach((c) => c.hub >= 0 && order.push(c.hub));
		for (let i = 0; i < this.count; i++) {
			if (d.hub[i]) order.push(i);
		}
		if (this.hover >= 0) order.unshift(this.hover);
		if (this.matches) for (const i of this.matches.slice(0, 40)) order.push(i);
		if (level !== 'hubs') {
			const extra: number[] = [];
			for (let i = 0; i < this.count; i++) {
				if (
					shouldLabel(level, {
						hub: false,
						bottleneck: false,
						hovered: false,
						matched: false,
						degree: d.degree[i],
					})
				)
					extra.push(i);
			}
			extra.sort((a, b) => d.degree[b] - d.degree[a]);
			for (const i of extra.slice(0, level === 'all' ? 260 : 90)) order.push(i);
		}
		const seen = new Set<number>();
		let placed = 0;
		for (const i of order) {
			if (seen.has(i)) continue;
			seen.add(i);
			if (this.alpha[i] < 0.5 || (d.orphan[i] && !this.showOrphans)) continue;
			if (this.focusNode && !this.focusNode[i] && i !== d.bottleneck) continue;
			const r = Math.max(2, d.radius[i] * Math.pow(k, 0.5) * 1.35);
			if (i === d.bottleneck) {
				const text = `${d.nodes[i].key ?? d.labels[i]} · blocks ${
					d.bottleneckBlocks
				}`;
				for (const dy of [4, -12, 20, -28, 36]) {
					if (
						put(
							text,
							sx[i] + r + 12,
							sy[i] + dy,
							12,
							600,
							dark ? '#ffb48a' : '#c2410c',
							dy === 36,
							'left',
						)
					)
						break;
				}
				continue;
			}
			const must = i === this.hover || !!this.matchSet?.[i];
			const color = th.ink;
			if (
				put(d.labels[i], sx[i], sy[i] + r + 14, 12, 600, color, must) &&
				++placed > 320
			)
				break;
		}
	}

	private nodeAt(px: number, py: number): number {
		const d = this.data;
		if (!d || !this.hasPos) return -1;
		let best = -1;
		let bestScore = Infinity;
		const scale = Math.pow(this.view.k, 0.5) * 1.35;
		for (let i = 0; i < this.count; i++) {
			if (this.alpha[i] < 0.5) continue;
			const r = Math.max(2, d.radius[i] * scale) + 5;
			const dx = this.sx[i] - px;
			const dy = this.sy[i] - py;
			const dist = dx * dx + dy * dy;
			if (dist > r * r) continue;
			if (dist / (r * r) < bestScore) {
				bestScore = dist / (r * r);
				best = i;
			}
		}
		return best;
	}

	private clusterAt(px: number, py: number): number {
		let best = -1;
		let bestScore = Infinity;
		this.geo.forEach((g, ci) => {
			if (!g.visible) return;
			const score = Math.hypot(px - g.x, py - g.y) / g.r;
			const onLabel = Math.abs(px - g.x) < 70 && Math.abs(py - g.top) < 14;
			if ((score <= 1 || onLabel) && score < bestScore) {
				bestScore = score;
				best = ci;
			}
		});
		return best;
	}

	private local(e: PointerEvent | WheelEvent | MouseEvent) {
		const rect = this.canvas.getBoundingClientRect();
		return { x: e.clientX - rect.left, y: e.clientY - rect.top };
	}

	private onDown = (e: PointerEvent) => {
		this.canvas.setPointerCapture?.(e.pointerId);
		const p = this.local(e);
		this.pointers.set(e.pointerId, p);
		if (this.pointers.size === 2) {
			const [a, b] = [...this.pointers.values()];
			this.pinch = Math.hypot(a.x - b.x, a.y - b.y);
			this.press = null;
			return;
		}
		this.press = { x: p.x, y: p.y, moved: false };
	};

	private onMove = (e: PointerEvent) => {
		const p = this.local(e);
		if (this.pointers.has(e.pointerId)) {
			const prev = this.pointers.get(e.pointerId)!;
			this.pointers.set(e.pointerId, p);
			if (this.pinch !== null && this.pointers.size === 2) {
				const [a, b] = [...this.pointers.values()];
				const dist = Math.hypot(a.x - b.x, a.y - b.y);
				this.autoFit = false;
				this.zoomAt((a.x + b.x) / 2, (a.y + b.y) / 2, dist / this.pinch);
				this.pinch = dist;
				return;
			}
			if (this.press) {
				if (
					!this.press.moved &&
					Math.hypot(p.x - this.press.x, p.y - this.press.y) > 4
				)
					this.press.moved = true;
				if (this.press.moved) {
					this.autoFit = false;
					this.anim = null;
					this.view = {
						...this.view,
						x: this.view.x + p.x - prev.x,
						y: this.view.y + p.y - prev.y,
					};
					this.canvas.style.cursor = 'grabbing';
					this.wake();
				}
			}
			return;
		}
		const i = this.nodeAt(p.x, p.y);
		const c = i < 0 ? this.clusterAt(p.x, p.y) : -1;
		this.canvas.style.cursor = i >= 0 || c >= 0 ? 'pointer' : 'grab';
		if (i !== this.hover) {
			this.hover = i;
			this.handlers.onHover(i >= 0 ? i : null);
			this.rebuildFocus();
		}
	};

	private onUp = (e: PointerEvent) => {
		const p = this.local(e);
		const press = this.press;
		this.pointers.delete(e.pointerId);
		if (this.pointers.size < 2) this.pinch = null;
		this.press = null;
		this.canvas.style.cursor = 'grab';
		if (!press || press.moved || e.type === 'pointercancel') return;
		const i = this.nodeAt(p.x, p.y);
		if (i >= 0) {
			this.handlers.onSelect(this.data!.nodes[i].id);
			return;
		}
		const c = this.clusterAt(p.x, p.y);
		if (c >= 0) this.focusCluster(c);
		else if (this.pinned) this.pin(null);
	};

	private onLeave = () => {
		if (this.hover >= 0) {
			this.hover = -1;
			this.handlers.onHover(null);
			this.rebuildFocus();
		}
	};

	private onDbl = (e: MouseEvent) => {
		const p = this.local(e);
		if (this.nodeAt(p.x, p.y) < 0 && this.clusterAt(p.x, p.y) < 0) this.fit();
	};

	private onWheel = (e: WheelEvent) => {
		e.preventDefault();
		const p = this.local(e);
		this.autoFit = false;
		const delta = e.deltaMode === 1 ? e.deltaY * 16 : e.deltaY;
		this.zoomAt(p.x, p.y, Math.exp(-delta * (e.ctrlKey ? 0.01 : 0.0016)));
	};
}
