import { setTourTelemetry } from '../telemetry';
import type { TriggerDeps } from '../triggers';
import { COOLDOWN_MS, createTriggers } from '../triggers';
import type { OnboardingState, TourDef, TourState } from '../types';

const memory = () => {
	const data = new Map<string, string>();
	return {
		getItem: (k: string) => data.get(k) ?? null,
		setItem: (k: string, v: string) => void data.set(k, v),
	};
};

const tour = (
	id: string,
	route: string,
	extra: Partial<TourDef['triggers'][number]> = {},
): TourDef => ({
	id,
	version: 1,
	since: '1',
	group: 'start',
	title: id,
	minutes: 1,
	triggers: [
		{ kind: 'first_visit', routeName: route, ...extra } as any,
		{ kind: 'hub' },
	],
	steps: [{ id: 's', type: 'spot', anchor: 'a', title: 't', body: 'b' }],
});

const onboarding = (extra: Partial<OnboardingState> = {}): OnboardingState => ({
	intent: null,
	tips_mode: 'auto',
	hub_hidden: false,
	checklist_hidden: false,
	tours_generation: 0,
	tours: {},
	facts: {},
	...extra,
});

const setup = (
	over: Partial<TriggerDeps> = {},
	tours = [tour('a.tour', 'List'), tour('b.tour', 'Board')],
) => {
	let now = 5_000_000;
	const states: Record<string, TourState> = {};
	const timers: Array<() => void> = [];
	const start = jest.fn().mockResolvedValue(true);
	const track = jest.fn();
	setTourTelemetry({ track });
	let ob: OnboardingState | null = onboarding();
	const deps: TriggerDeps = {
		tours: () => tours,
		onboarding: () => ob,
		tourState: (id) => states[id] ?? null,
		now: () => now,
		storage: memory(),
		session: memory(),
		isBusy: () => false,
		hasActive: () => false,
		eligible: () => true,
		start,
		setTimer: (fn) => {
			timers.push(fn);
			return () => {
				const i = timers.indexOf(fn);
				if (i >= 0) timers.splice(i, 1);
			};
		},
		...over,
	};
	const triggers = createTriggers(deps);
	return {
		triggers,
		start,
		track,
		states,
		deps,
		setOb: (o: OnboardingState | null) => (ob = o),
		advance: (ms: number) => (now += ms),
		fire: () => timers.splice(0).forEach((f) => f()),
		visit: (route: string) => {
			triggers.onRouteEntered(route);
			timers.splice(0).forEach((f) => f());
		},
	};
};

describe('triggers', () => {
	it('starts the first_visit tour after the route settles and tracks the offer', () => {
		const h = setup();
		h.visit('List');
		expect(h.start).toHaveBeenCalledWith('a.tour', 'first_visit');
		expect(h.track).toHaveBeenCalledWith('tour_offered', {
			tour: 'a.tour',
			trigger: 'first_visit',
		});
	});

	it('only one auto tour per session', () => {
		const h = setup();
		h.visit('List');
		h.visit('Board');
		expect(h.start).toHaveBeenCalledTimes(1);
	});

	it('a route left before it settles cancels the attempt', () => {
		const h = setup();
		h.triggers.onRouteEntered('List');
		h.triggers.onRouteEntered('Other');
		h.fire();
		expect(h.start).not.toHaveBeenCalled();
	});

	it.each([['hub_only'], ['off']])(
		'tips_mode %s silences auto tours',
		(mode) => {
			const h = setup();
			h.setOb(onboarding({ tips_mode: mode as any }));
			h.visit('List');
			expect(h.start).not.toHaveBeenCalled();
		},
	);

	it('does nothing before onboarding state is known', () => {
		const h = setup();
		h.setOb(null);
		h.visit('List');
		expect(h.start).not.toHaveBeenCalled();
	});

	it('not while an input or modal is focused, or another tour is active', () => {
		let busy = true;
		const h = setup({ isBusy: () => busy });
		h.visit('List');
		expect(h.start).not.toHaveBeenCalled();
		busy = false;
		h.visit('List');
		expect(h.start).toHaveBeenCalledTimes(1);
		const g = setup({ hasActive: () => true });
		g.visit('List');
		expect(g.start).not.toHaveBeenCalled();
	});

	it('24h cooldown after any skip', () => {
		const h = setup();
		h.triggers.noteSkip('x.tour');
		h.visit('List');
		expect(h.start).not.toHaveBeenCalled();
		h.advance(COOLDOWN_MS - 1);
		h.visit('List');
		expect(h.start).not.toHaveBeenCalled();
		h.advance(2);
		h.visit('List');
		expect(h.start).toHaveBeenCalledTimes(1);
	});

	it('a tour skipped twice is not auto-offered; skipped once is offered again later', () => {
		const h = setup();
		h.states['a.tour'] = {
			status: 'skipped',
			step_id: 's',
			version: 1,
			generation: 0,
		};
		h.triggers.noteSkip('a.tour');
		h.advance(COOLDOWN_MS + 1);
		h.visit('List');
		expect(h.start).toHaveBeenCalledTimes(1);

		const g = setup();
		g.states['a.tour'] = {
			status: 'skipped',
			step_id: 's',
			version: 1,
			generation: 0,
		};
		g.triggers.noteSkip('a.tour');
		g.triggers.noteSkip('a.tour');
		g.advance(COOLDOWN_MS + 1);
		g.visit('List');
		expect(g.start).not.toHaveBeenCalled();
		expect(g.triggers.skipCount('a.tour')).toBe(2);
	});

	it.each(['completed', 'paused', 'in_progress'] as const)(
		'never auto-offers a %s tour',
		(status) => {
			const h = setup();
			h.states['a.tour'] = { status, step_id: 's', version: 1, generation: 0 };
			h.visit('List');
			expect(h.start).not.toHaveBeenCalled();
		},
	);

	it('honours unlessFact and eligibility', () => {
		const h = setup({}, [tour('a.tour', 'List', { unlessFact: 'has_task' })]);
		h.setOb(onboarding({ facts: { has_task: true } }));
		h.visit('List');
		expect(h.start).not.toHaveBeenCalled();
		const g = setup({ eligible: () => false });
		g.visit('List');
		expect(g.start).not.toHaveBeenCalled();
	});

	it('hub-only tours are never auto-started', () => {
		const t = tour('h.tour', 'List');
		t.triggers = [{ kind: 'hub' }];
		const h = setup({}, [t]);
		h.visit('List');
		expect(h.start).not.toHaveBeenCalled();
	});

	it.each(['hub_only', 'off'] as const)(
		'tips_mode %s never auto-starts a tour',
		(mode) => {
			const h = setup();
			h.setOb(onboarding({ tips_mode: mode }));
			h.visit('List');
			h.visit('Board');
			expect(h.start).not.toHaveBeenCalled();
			expect(h.track).not.toHaveBeenCalledWith(
				'tour_offered',
				expect.anything(),
			);
		},
	);

	it('unlessFact starts the tour while the fact is false', () => {
		const h = setup({}, [tour('a.tour', 'List', { unlessFact: 'has_task' })]);
		h.setOb(onboarding({ facts: { has_task: false } }));
		h.visit('List');
		expect(h.start).toHaveBeenCalledWith('a.tour', 'first_visit');
	});

	it('does not consume the session slot when the tour fails to start', async () => {
		const h = setup({ start: jest.fn().mockResolvedValue(false) });
		h.visit('List');
		await Promise.resolve();
		await Promise.resolve();
		expect(h.triggers.canAutoOffer(tour('a.tour', 'List'))).toBe(true);
	});

	it('keeps the session slot once the tour started', async () => {
		const h = setup();
		h.visit('List');
		await Promise.resolve();
		await Promise.resolve();
		expect(h.triggers.canAutoOffer(tour('b.tour', 'Board'))).toBe(false);
	});
});
