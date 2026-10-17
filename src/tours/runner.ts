import { pickAnchorName } from './anchors';
import type { TourBus, TourEventNameKey } from './events';
import { KNOWN_EVENTS } from './events';
import type { SaveTourInput } from './store';
import { tourTelemetry } from './telemetry';
import type { StepDef, TourDef, TourId, TourState } from './types';
import type { ActiveTour } from './useTours';

export const MIN_TIMER_MS = 60_000;
export const ALREADY_DONE_MS = 1500;

export interface RunnerView extends ActiveTour {
	anchorEl: Element | null;
	mode: 'modal' | 'passive' | 'centered';
}

export type RunnerRoute = NonNullable<StepDef['route']>;
export type FinishReason =
	| 'completed'
	| 'skipped'
	| 'paused'
	| 'module_off'
	| 'navigated';

export interface RunnerDeps {
	getTour(id: TourId): TourDef | undefined;
	stateOf(id: TourId): TourState | null;
	now(): number;
	mobile(): boolean;
	modulesKnown(): boolean;
	isModuleEnabled(key: string): boolean;
	currentRouteName(): string | null;
	currentRouteQuery?(): Record<string, unknown>;
	navigate(route: RunnerRoute): void;
	resolveAnchor(
		step: StepDef,
		options: {
			mobile: boolean;
			eventTaskId: number | null;
			signal: AbortSignal;
		},
	): Promise<Element | null>;
	waitForAnchor?(
		step: StepDef,
		options: {
			mobile: boolean;
			eventTaskId: number | null;
			signal: AbortSignal;
		},
	): Promise<Element | null>;
	facts(): Record<string, unknown>;
	save(id: TourId, input: SaveTourInput): void;
	bus: TourBus;
	render(view: RunnerView | null): void;
	setTimer(fn: () => void, ms: number): () => void;
	eligible?(tour: TourDef): boolean;
	onFinished?(id: TourId, reason: FinishReason): void;
	onSkip?(id: TourId): void;
}

export function durBucket(ms: number): 'lt_1m' | 'm1_3' | 'm3_10' | 'gt_10m' {
	if (ms < 60_000) return 'lt_1m';
	if (ms < 180_000) return 'm1_3';
	if (ms < 600_000) return 'm3_10';
	return 'gt_10m';
}

interface Current {
	tour: TourDef;
	idx: number;
	startedAt: number;
	completedEmitted: boolean;
	alreadyDone: boolean;
	unavailable: boolean;
	anchorEl: Element | null;
	hint?: 'timer_too_short';
}

const whereMatches = (
	where: Record<string, string | number> | undefined,
	payload: Record<string, unknown>,
) =>
	!where ||
	Object.entries(where).every(([key, value]) => payload[key] === value);

export class TourRunner {
	private cur: Current | null = null;
	private abort: AbortController | null = null;
	private cancelTimer: (() => void) | null = null;
	private token = 0;
	private eventTaskId: number | null = null;
	private timerStarts = new Map<number, number>();
	private seen = new Set<string>();
	private navigatedAt = -Infinity;
	private renavigated = false;
	private routeReached = false;
	private offBus: Array<() => void> = [];
	private listening = new Set<string>();

	constructor(private deps: RunnerDeps) {
		for (const name of KNOWN_EVENTS) this.listen(name);
	}

	private listen(name: TourEventNameKey): void {
		if (this.listening.has(name)) return;
		this.listening.add(name);
		this.offBus.push(
			this.deps.bus.on(name, (payload) => void this.onEvent(name, payload)),
		);
	}

	private listenForTour(tour: TourDef): void {
		const names = [tour.completeOn, ...tour.steps.map((s) => s.waitFor?.event)];
		for (const name of names) if (name) this.listen(name as TourEventNameKey);
	}

	dispose(): void {
		this.offBus.forEach((off) => off());
		this.offBus = [];
		this.listening.clear();
		this.cancelPending();
		this.cur = null;
	}

	get activeTourId(): TourId | null {
		return this.cur?.tour.id ?? null;
	}

	async start(
		id: TourId,
		trigger: string,
		fromStepId?: string | null,
	): Promise<boolean> {
		const tour = this.deps.getTour(id);
		if (!tour || tour.steps.length === 0) return false;
		if (this.deps.eligible && !this.deps.eligible(tour)) return false;
		if (!this.modulesOk(tour)) return false;
		this.listenForTour(tour);
		if (this.cur) this.pause();
		const found = fromStepId
			? tour.steps.findIndex((s) => s.id === fromStepId)
			: -1;
		const idx = found >= 0 ? found : 0;
		this.cur = {
			tour,
			idx,
			startedAt: this.deps.now(),
			completedEmitted: false,
			alreadyDone: false,
			unavailable: false,
			anchorEl: null,
		};
		tourTelemetry().track('tour_started', { tour: id, trigger });
		await this.enterStep(idx);
		return true;
	}

	async resume(id: TourId): Promise<boolean> {
		const saved = this.deps.stateOf(id);
		const resumable =
			saved && (saved.status === 'paused' || saved.status === 'in_progress');
		return this.start(id, 'resume', resumable ? saved.step_id : null);
	}

	pause(reason: FinishReason = 'paused'): void {
		if (!this.cur) return;
		const { tour, idx } = this.cur;
		tourTelemetry().track('tour_paused', { tour: tour.id, i: idx + 1 });
		this.deps.save(tour.id, {
			status: 'paused',
			step_id: tour.steps[idx].id,
			version: tour.version,
		});
		this.finish(reason);
	}

	async next(): Promise<void> {
		const cur = this.cur;
		if (!cur || cur.unavailable) return;
		if (cur.tour.steps[cur.idx].type === 'wait' && !cur.alreadyDone) return;
		await this.advance('next');
	}

	async skipStep(): Promise<void> {
		const cur = this.cur;
		if (!cur) return;
		const step = cur.tour.steps[cur.idx];
		tourTelemetry().track('tour_step_skipped', {
			tour: cur.tour.id,
			step: step.id,
			i: cur.idx + 1,
		});
		this.deps.onSkip?.(cur.tour.id);
		await this.enterStep(cur.idx + 1);
	}

	skipTour(): void {
		const cur = this.cur;
		if (!cur) return;
		tourTelemetry().track('tour_skipped', {
			tour: cur.tour.id,
			i: cur.idx + 1,
		});
		this.deps.save(cur.tour.id, {
			status: 'skipped',
			step_id: cur.tour.steps[cur.idx].id,
			version: cur.tour.version,
		});
		this.deps.onSkip?.(cur.tour.id);
		this.finish('skipped');
	}

	async takeMeThere(): Promise<void> {
		const cur = this.cur;
		if (!cur) return;
		const step = cur.tour.steps[cur.idx];
		const route = step.takeMeRoute ?? step.route;
		if (!route) return;
		if (!this.onRoute(route)) {
			this.navigatedAt = this.deps.now();
			this.renavigated = false;
			this.routeReached = false;
			this.deps.navigate(route);
		}
		await this.enterStep(cur.idx);
	}

	abandon(): void {
		if (this.cur) this.finish('paused');
	}

	recheck(): void {
		const cur = this.cur;
		if (!cur || !this.deps.modulesKnown()) return;
		if (!this.modulesOk(cur.tour)) this.pause('module_off');
	}

	routeChanged(name: string | null): void {
		const cur = this.cur;
		if (!cur) return;
		const route = cur.tour.steps[cur.idx].route;
		if (!route) return;
		if (name === route.name && this.queryMatches(route)) return;
		if (this.deps.now() - this.navigatedAt < 3000) {
			if (!this.renavigated && !this.routeReached) {
				this.renavigated = true;
				this.deps.navigate(route);
			}
			return;
		}
		this.pause('navigated');
	}

	private onRoute(route: NonNullable<StepDef['route']>): boolean {
		return (
			this.deps.currentRouteName() === route.name && this.queryMatches(route)
		);
	}

	private queryMatches(route: NonNullable<StepDef['route']>): boolean {
		const current = this.deps.currentRouteQuery?.() ?? {};
		return Object.entries(route.query ?? {}).every(
			([key, value]) => current[key] === value,
		);
	}

	private modulesOk(tour: TourDef): boolean {
		if (!this.deps.modulesKnown()) return true;
		return (tour.requires?.modules ?? []).every((key) =>
			this.deps.isModuleEnabled(key),
		);
	}

	private stepModulesOk(step: StepDef): boolean {
		if (!this.deps.modulesKnown()) return true;
		return (step.requiresModules ?? []).every((key) =>
			this.deps.isModuleEnabled(key),
		);
	}

	private cancelPending(): void {
		this.abort?.abort();
		this.abort = null;
		this.cancelTimer?.();
		this.cancelTimer = null;
		this.token++;
	}

	private finish(reason: FinishReason): void {
		const cur = this.cur;
		this.cancelPending();
		this.cur = null;
		this.deps.render(null);
		if (cur) this.deps.onFinished?.(cur.tour.id, reason);
	}

	private async advance(via: 'next' | 'action'): Promise<void> {
		const cur = this.cur;
		if (!cur) return;
		const step = cur.tour.steps[cur.idx];
		tourTelemetry().track('tour_step_completed', {
			tour: cur.tour.id,
			step: step.id,
			i: cur.idx + 1,
			via,
		});
		await this.enterStep(cur.idx + 1);
	}

	private complete(): void {
		const cur = this.cur;
		if (!cur) return;
		this.markCompleted(cur);
		this.finish('completed');
	}

	private markCompleted(cur: Current): void {
		if (cur.completedEmitted) return;
		cur.completedEmitted = true;
		tourTelemetry().track('tour_completed', {
			tour: cur.tour.id,
			dur: durBucket(this.deps.now() - cur.startedAt),
		});
		this.deps.save(cur.tour.id, {
			status: 'completed',
			step_id: null,
			version: cur.tour.version,
		});
	}

	private async enterStep(from: number): Promise<void> {
		const cur = this.cur;
		if (!cur) return;
		this.cancelPending();
		const token = this.token;
		const { tour } = cur;
		let idx = from;
		while (idx < tour.steps.length) {
			const candidate = tour.steps[idx];
			if (!this.stepModulesOk(candidate)) {
				tourTelemetry().track('tour_step_unavailable', {
					tour: tour.id,
					step: candidate.id,
				});
			} else if (!this.skipped(candidate)) break;
			else this.trackSkipWhen(tour, candidate, idx);
			idx++;
		}
		if (idx >= tour.steps.length) return this.complete();

		const step = tour.steps[idx];
		cur.idx = idx;
		cur.alreadyDone = false;
		cur.unavailable = false;
		cur.anchorEl = null;
		cur.hint = undefined;
		if (!cur.completedEmitted) {
			this.deps.save(tour.id, {
				status: 'in_progress',
				step_id: step.id,
				version: tour.version,
			});
		}

		if (step.route && !this.onRoute(step.route)) {
			this.navigatedAt = this.deps.now();
			this.renavigated = false;
			this.routeReached = false;
			this.deps.navigate(step.route);
		}

		if (pickAnchorName(step, this.deps.mobile())) {
			this.abort = new AbortController();
			const anchor = await this.deps.resolveAnchor(step, {
				mobile: this.deps.mobile(),
				eventTaskId: this.eventTaskId,
				signal: this.abort.signal,
			});
			if (token !== this.token || this.cur !== cur) return;
			if (!anchor) {
				if (step.type === 'wait' && this.alreadyDone(step))
					return this.enterStep(idx + 1);
				tourTelemetry().track('tour_step_unavailable', {
					tour: tour.id,
					step: step.id,
				});
				if (step.optional) return this.enterStep(idx + 1);
				cur.unavailable = true;
				this.present(cur, 'centered');
				this.watchForAnchor(cur, step, token);
				return;
			}
			cur.anchorEl = anchor;
		}
		this.routeReached = true;

		const isWait = step.type === 'wait';
		if (isWait && this.alreadyDone(step)) {
			cur.alreadyDone = true;
			this.present(cur, 'passive');
			tourTelemetry().track('tour_step_viewed', {
				tour: tour.id,
				step: step.id,
				i: idx + 1,
			});
			this.cancelTimer = this.deps.setTimer(() => {
				if (token === this.token && this.cur === cur)
					void this.advance('action');
			}, ALREADY_DONE_MS);
			return;
		}
		this.present(cur, isWait ? 'passive' : cur.anchorEl ? 'modal' : 'centered');
		tourTelemetry().track('tour_step_viewed', {
			tour: tour.id,
			step: step.id,
			i: idx + 1,
		});
	}

	private watchForAnchor(cur: Current, step: StepDef, token: number): void {
		if (!this.deps.waitForAnchor) return;
		this.abort = new AbortController();
		void this.deps
			.waitForAnchor(step, {
				mobile: this.deps.mobile(),
				eventTaskId: this.eventTaskId,
				signal: this.abort.signal,
			})
			.then((anchor) => {
				if (anchor && token === this.token && this.cur === cur)
					void this.enterStep(cur.idx);
			});
	}

	factsChanged(): void {
		const cur = this.cur;
		if (!cur) return;
		const step = cur.tour.steps[cur.idx];
		if (this.skipped(step)) {
			this.trackSkipWhen(cur.tour, step, cur.idx);
			void this.enterStep(cur.idx + 1);
			return;
		}
		if (step.type === 'wait' && !cur.alreadyDone && this.alreadyDone(step)) {
			void (cur.unavailable ? this.enterStep(cur.idx) : this.advance('action'));
		}
	}

	private trackSkipWhen(tour: TourDef, step: StepDef, idx: number): void {
		tourTelemetry().track('tour_step_skipped', {
			tour: tour.id,
			step: step.id,
			i: idx + 1,
		});
	}

	private skipped(step: StepDef): boolean {
		return !!step.skipWhen && !!this.deps.facts()[step.skipWhen];
	}

	private alreadyDone(step: StepDef): boolean {
		if (!step.alreadyDone) return false;
		return (
			!!this.deps.facts()[step.alreadyDone] || this.seen.has(step.alreadyDone)
		);
	}

	private present(cur: Current, mode: RunnerView['mode']): void {
		const step = cur.tour.steps[cur.idx];
		const mobile = this.deps.mobile();
		const target = step.takeMeRoute ?? step.route;
		const onTarget = !!target && this.onRoute(target);
		this.deps.render({
			tour: cur.tour,
			step,
			index: cur.idx,
			total: cur.tour.steps.length,
			waiting: step.type === 'wait' && !cur.alreadyDone && !cur.unavailable,
			unavailable: cur.unavailable,
			alreadyDone: cur.alreadyDone,
			canTakeMe: cur.unavailable && !!target && !onTarget,
			onTargetPage: cur.unavailable && !!target && onTarget,
			mobile,
			hint: cur.hint,
			anchorEl: cur.anchorEl,
			mode,
		});
	}

	private timerLongEnough(payload: Record<string, unknown>): boolean {
		const taskId = Number(payload.taskId);
		const started = this.timerStarts.get(taskId);
		this.timerStarts.delete(taskId);
		return started === undefined || this.deps.now() - started >= MIN_TIMER_MS;
	}

	private async onEvent(
		name: string,
		payload: Record<string, unknown>,
	): Promise<void> {
		this.seen.add(name);
		if (name === 'task.created' && payload.taskId != null)
			this.eventTaskId = Number(payload.taskId);
		if (name === 'timer.started')
			this.timerStarts.set(Number(payload.taskId), this.deps.now());
		const longEnough =
			name === 'timer.stopped' ? this.timerLongEnough(payload) : true;

		const cur = this.cur;
		if (!cur) return;
		const step = cur.tour.steps[cur.idx];
		const waits =
			step.type === 'wait' &&
			!cur.unavailable &&
			!cur.alreadyDone &&
			step.waitFor?.event === name &&
			whereMatches(step.waitFor.where, payload);

		if (!longEnough) {
			if (waits) {
				cur.hint = 'timer_too_short';
				this.present(cur, 'passive');
			}
			return;
		}
		if (cur.tour.completeOn === name) this.markCompleted(cur);
		if (waits) await this.advance('action');
	}
}
