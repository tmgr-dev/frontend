import type { TourBus } from '@/tours/events';
import { tourBus } from '@/tours/events';
import type { Ref } from 'vue';
import { ref } from 'vue';

export type PollerState = 'polling' | 'stalled' | 'exhausted' | 'done';

export type Facts = Record<string, unknown>;

export interface FactsPollerDeps {
	fetchFacts(keys: string[], taskId?: number | string): Promise<Facts>;
	keys: string[];
	taskId?: number | string;
	now(): number;
	setTimer(fn: () => void, ms: number): () => void;
	visibility: { hidden(): boolean; onChange(fn: () => void): () => void };
	onFacts?(facts: Facts): void;
	stallAfterMs?: number;
	onStall?(): void;
	onExhausted?(): void;
	inert?(): boolean;
	bus?: Pick<TourBus, 'emit'>;
}

export interface FactsPoller {
	start(): void;
	stop(): void;
	checkAgain(): void;
	state: Ref<PollerState>;
}

const MINUTE = 60_000;

export function intervalFor(elapsedMs: number): number | null {
	if (elapsedMs < MINUTE) return 5_000;
	if (elapsedMs < 5 * MINUTE) return 15_000;
	if (elapsedMs < 10 * MINUTE) return 30_000;
	return null;
}

export function createFactsPoller(deps: FactsPollerDeps): FactsPoller {
	const state = ref<PollerState>('done');
	const stallAfter = deps.stallAfterMs ?? 3 * MINUTE;
	const bus = deps.bus ?? tourBus;
	let generation = 0;
	let pendingGeneration = -1;
	let active = false;
	let stopped = false;
	let startedAt = 0;
	let announced = false;
	let cancelPoll: (() => void) | null = null;
	let cancelStall: (() => void) | null = null;
	let offVisibility: (() => void) | null = null;

	const clearTimers = () => {
		cancelPoll?.();
		cancelPoll = null;
		cancelStall?.();
		cancelStall = null;
	};

	const finish = (next: PollerState) => {
		active = false;
		clearTimers();
		offVisibility?.();
		offVisibility = null;
		state.value = next;
	};

	const exhaust = () => {
		finish('exhausted');
		deps.onExhausted?.();
	};

	const schedule = () => {
		const interval = intervalFor(deps.now() - startedAt);
		if (interval === null) return exhaust();
		cancelPoll = deps.setTimer(() => {
			cancelPoll = null;
			void tick();
		}, interval);
	};

	const tick = async () => {
		if (!active || deps.visibility.hidden() || pendingGeneration === generation)
			return;
		const mine = generation;
		pendingGeneration = mine;
		let facts: Facts | null = null;
		try {
			facts = await deps.fetchFacts(deps.keys, deps.taskId);
		} catch {
			facts = null;
		}
		if (mine !== generation || !active) return;
		pendingGeneration = -1;
		if (facts) {
			deps.onFacts?.(facts);
			if (
				!announced &&
				deps.keys.includes('mcp_first_call_at') &&
				facts.mcp_first_call_at
			) {
				announced = true;
				bus.emit('agent.first_mcp_call');
			}
			if (deps.keys.every((key) => !!facts![key])) return finish('done');
		}
		schedule();
	};

	const onVisibility = () => {
		if (!active) return;
		cancelPoll?.();
		cancelPoll = null;
		if (deps.visibility.hidden()) return;
		if (intervalFor(deps.now() - startedAt) === null) return exhaust();
		void tick();
	};

	const begin = () => {
		clearTimers();
		offVisibility?.();
		generation += 1;
		active = true;
		stopped = false;
		startedAt = deps.now();
		state.value = 'polling';
		offVisibility = deps.visibility.onChange(onVisibility);
		cancelStall = deps.setTimer(() => {
			cancelStall = null;
			if (!active || state.value !== 'polling') return;
			state.value = 'stalled';
			deps.onStall?.();
		}, stallAfter);
		void tick();
	};

	return {
		state,
		start() {
			if (deps.inert?.()) {
				state.value = 'exhausted';
				deps.onExhausted?.();
				return;
			}
			begin();
		},
		stop() {
			stopped = true;
			active = false;
			generation += 1;
			clearTimers();
			offVisibility?.();
			offVisibility = null;
		},
		checkAgain() {
			if (stopped || state.value === 'done' || deps.inert?.()) return;
			begin();
		},
	};
}
