import { t } from '@/i18n';

export const STREAK_MILESTONES = [5, 10, 25, 50, 100] as const;
export const STREAK_GAP_MS = 10 * 60 * 1000;
export const SPEEDRUN_MS = 3000;
export const SPEEDRUN_COOLDOWN_MS = 60 * 1000;
export const NIGHT_OWL_FROM = 2;
export const NIGHT_OWL_TO = 5;
export const DOOM_AWAITS_AT = 10;
export const RARE_ODDS = 50;
export const LIFETIME_MILESTONES = [100, 500, 1000] as const;
export const KONAMI = [
	'ArrowUp',
	'ArrowUp',
	'ArrowDown',
	'ArrowDown',
	'ArrowLeft',
	'ArrowRight',
	'ArrowLeft',
	'ArrowRight',
	'b',
	'a',
] as const;

export const DEFAULT_SUBTITLE = 'decisions.subtitle.default';
export const RARE_LINES = [
	'decisions.subtitle.rare.scrollingUseful',
	'decisions.subtitle.rare.oneMoreCard',
	'decisions.subtitle.rare.optionalDoom',
	'decisions.subtitle.rare.solvingNotScrolling',
	'decisions.subtitle.rare.touchGrass',
	'decisions.subtitle.rare.notProcrastination',
	'decisions.subtitle.rare.hello',
] as const;

export interface EggState {
	streak: number;
	lastAnswerAt: number;
	total: number;
	inboxZeroDay: string;
	nightOwlDay: string;
	best: number;
}

export const emptyEggState = (): EggState => ({
	streak: 0,
	lastAnswerAt: 0,
	total: 0,
	inboxZeroDay: '',
	nightOwlDay: '',
	best: 0,
});

export const advanceStreak = (
	state: EggState,
	now: number,
	gapMs: number = STREAK_GAP_MS,
): EggState => {
	const continues =
		state.lastAnswerAt > 0 &&
		now >= state.lastAnswerAt &&
		now - state.lastAnswerAt <= gapMs;
	const streak = continues ? state.streak + 1 : 1;
	return {
		...state,
		streak,
		lastAnswerAt: now,
		total: state.total + 1,
		best: Math.max(state.best, streak),
	};
};

export const streakMilestone = (streak: number): number | null =>
	(STREAK_MILESTONES as readonly number[]).includes(streak) ? streak : null;

export const lifetimeMilestone = (total: number): number | null =>
	(LIFETIME_MILESTONES as readonly number[]).includes(total) ? total : null;

export const isSpeedrun = (
	openedAt: number,
	answeredAt: number,
	thresholdMs: number = SPEEDRUN_MS,
): boolean => {
	const took = answeredAt - openedAt;
	return openedAt > 0 && took >= 0 && took <= thresholdMs;
};

export const speedrunReady = (
	lastFiredAt: number,
	now: number,
	cooldownMs: number = SPEEDRUN_COOLDOWN_MS,
): boolean => lastFiredAt <= 0 || now - lastFiredAt >= cooldownMs;

export const isNightOwl = (date: Date): boolean => {
	const hour = date.getHours();
	return hour >= NIGHT_OWL_FROM && hour < NIGHT_OWL_TO;
};

export const dayKey = (date: Date): string => {
	const pad = (n: number) => String(n).padStart(2, '0');
	return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(
		date.getDate(),
	)}`;
};

export const inboxZeroToday = (
	lastDay: string,
	now: Date,
): { fire: boolean; day: string } => {
	const day = dayKey(now);
	return { fire: lastDay !== day, day };
};

export const advanceKonami = (
	position: number,
	key: string,
): { position: number; done: boolean } => {
	const normalized = key.length === 1 ? key.toLowerCase() : key;
	if (normalized === KONAMI[position]) {
		const next = position + 1;
		return next === KONAMI.length
			? { position: 0, done: true }
			: { position: next, done: false };
	}
	const seen = [...KONAMI.slice(0, position), normalized];
	for (let k = Math.min(position, KONAMI.length - 1); k > 0; k -= 1) {
		const tail = seen.slice(seen.length - k);
		if (tail.every((value, i) => value === KONAMI[i]))
			return { position: k, done: false };
	}
	return { position: 0, done: false };
};

export const pickSubtitle = (
	random: () => number = Math.random,
	odds: number = RARE_ODDS,
): string => {
	if (random() >= 1 / odds) return DEFAULT_SUBTITLE;
	const index = Math.min(
		RARE_LINES.length - 1,
		Math.floor(random() * RARE_LINES.length),
	);
	return RARE_LINES[index];
};

export const doomAwaits = (pending: number): boolean =>
	pending >= DOOM_AWAITS_AT;

export const streakMessage = (streak: number): string =>
	t('decisions.doom.streak', { n: streak });

const DOOM_SLOT = 'tmgr.decisions.doom.v1';

interface StorageLike {
	getItem(key: string): string | null;
	setItem(key: string, value: string): void;
}

const defaultStorage = (): StorageLike | null => {
	try {
		return window.localStorage;
	} catch {
		return null;
	}
};

export const loadEggState = (
	storage: StorageLike | null = defaultStorage(),
): EggState => {
	const base = emptyEggState();
	try {
		const raw = storage?.getItem(DOOM_SLOT);
		if (!raw) return base;
		const data = JSON.parse(raw) as Partial<EggState>;
		const num = (v: unknown) =>
			typeof v === 'number' && Number.isFinite(v) && v >= 0 ? v : 0;
		return {
			streak: num(data.streak),
			lastAnswerAt: num(data.lastAnswerAt),
			total: num(data.total),
			best: num(data.best),
			inboxZeroDay:
				typeof data.inboxZeroDay === 'string' ? data.inboxZeroDay : '',
			nightOwlDay: typeof data.nightOwlDay === 'string' ? data.nightOwlDay : '',
		};
	} catch {
		return base;
	}
};

export const saveEggState = (
	state: EggState,
	storage: StorageLike | null = defaultStorage(),
): void => {
	try {
		storage?.setItem(DOOM_SLOT, JSON.stringify(state));
	} catch {
		return;
	}
};
