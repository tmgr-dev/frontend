import type { CardView } from '@/decisions/types';
import type {
	PopoverExtras,
	PopoverModel,
	PopoverTask,
	UsageBar,
} from './types';

export type UsageLevel = 'ok' | 'warn' | 'crit';
export type DndOption = PopoverModel['dnd']['option'];
export type PopoverSection =
	'decisions' | 'missed' | 'switch' | 'today' | 'machines' | 'ai' | 'plugins';
export type PopoverUsage = NonNullable<PopoverExtras['usage']>;

const DAY = 86400;

const pad = (value: number): string => String(value).padStart(2, '0');

export const usageLevel = (pct: number): UsageLevel => {
	if (pct >= 95) return 'crit';
	return pct > 70 ? 'warn' : 'ok';
};

export const durationText = (seconds: number): string => {
	const minutes = Math.max(0, Math.floor(seconds / 60));
	const hours = Math.floor(minutes / 60);
	const rest = minutes % 60;
	return hours > 0 ? `${hours}h ${pad(rest)}m` : `${rest}m`;
};

export const resetText = (
	resetsAt: number | null,
	now: number,
): string | null => {
	if (resetsAt === null) return null;
	const left = resetsAt - now;
	if (left > DAY) {
		return new Date(resetsAt * 1000).toLocaleDateString('en-US', {
			weekday: 'short',
		});
	}
	return durationText(left);
};

export const formatClock = (seconds: number): string => {
	const total = Math.max(0, Math.floor(seconds));
	const hours = Math.floor(total / 3600);
	const minutes = Math.floor((total % 3600) / 60);
	return `${hours}:${pad(minutes)}:${pad(total % 60)}`;
};

export const tokensText = (tokens: number): string => {
	if (tokens >= 1_000_000) return `${(tokens / 1_000_000).toFixed(1)}M`;
	if (tokens >= 1_000) return `${Math.round(tokens / 1_000)}K`;
	return String(tokens);
};

export const moneyText = (amount: number): string => `$${amount.toFixed(2)}`;

export const ageText = (createdMs: number, nowMs: number): string => {
	const minutes = Math.max(0, Math.floor((nowMs - createdMs) / 60000));
	if (minutes < 60) return `${minutes}m`;
	const hours = Math.floor(minutes / 60);
	return hours < 24 ? `${hours}h` : `${Math.floor(hours / 24)}d`;
};

export const runningElapsed = (task: PopoverTask, nowSec: number): number => {
	const running = task.startTime > 0 ? nowSec - task.startTime : 0;
	return Math.max(0, task.commonTime + running);
};

export const ticketCode = (title: string): string | null => {
	const split = title.indexOf(': ');
	if (split < 0) return null;
	const code = title.slice(0, split);
	const dash = code.indexOf('-');
	if (dash < 0) return null;
	const prefix = code.slice(0, dash);
	const number = code.slice(dash + 1);
	const valid = /^[A-Z0-9]+$/.test(prefix) && /^[0-9]+$/.test(number);
	return valid ? code : null;
};

export const stripTicket = (title: string): string => {
	const code = ticketCode(title);
	return code ? title.slice(code.length + 2) : title;
};

export const nextIndex = (
	current: number,
	count: number,
	direction: 1 | -1,
): number => {
	if (count <= 0) return -1;
	if (current < 0) return direction > 0 ? 0 : count - 1;
	return (current + direction + count) % count;
};

export const orderOptions = <T extends { recommended: boolean }>(
	options: readonly T[],
): T[] =>
	[...options].sort((a, b) => Number(b.recommended) - Number(a.recommended));

export const nextResize = (
	measured: number,
	max: number,
	last: number,
): number | null => {
	const height = Math.min(Math.ceil(measured), max);
	return height === last ? null : height;
};

export const taskKey = (workspaceId: number, id: number): string =>
	`${workspaceId}:${id}`;

export const actionIds = {
	open: (workspaceId: number, id: number) => `open:${taskKey(workspaceId, id)}`,
	stop: (workspaceId: number, id: number) => `stop:${taskKey(workspaceId, id)}`,
	switch: (workspaceId: number, id: number) =>
		`switch:${taskKey(workspaceId, id)}`,
	dnd: (option: DndOption) => `dnd:${option}`,
	routineDone: (id: number) => `routine-done:${id}`,
	pluginItem: (id: string) => `plugin-item:${id}`,
};

export const isPending = (card: CardView): boolean =>
	card.state === 'open' || card.state === 'deferred';

export const isAnswerable = (card: CardView): boolean =>
	isPending(card) &&
	!card.action &&
	card.options.length > 0 &&
	!(
		card.reply_via === 'hook' &&
		!card.source.runner_session &&
		card.push !== true
	);

const MAC_SYMBOLS: Record<string, string> = {
	Command: '⌘',
	Cmd: '⌘',
	Super: '⌘',
	Control: '⌃',
	Ctrl: '⌃',
	Alt: '⌥',
	Option: '⌥',
	Shift: '⇧',
};

const WIN_LABELS: Record<string, string> = {
	Command: 'Ctrl',
	CommandOrControl: 'Ctrl',
	CmdOrCtrl: 'Ctrl',
	Control: 'Ctrl',
	Super: 'Win',
};

export const hotkeyText = (
	accelerator: string | null | undefined,
	platform: PopoverModel['platform'],
): string => {
	if (!accelerator) return '';
	const parts = accelerator.split('+');
	if (platform === 'macos') {
		return parts.map((part) => MAC_SYMBOLS[part] ?? part).join('');
	}
	return parts.map((part) => WIN_LABELS[part] ?? part).join('+');
};

export const pendingDecisions = (cards: readonly CardView[]): CardView[] =>
	cards.filter(isPending);

export const hasUsage = (usage: PopoverUsage | undefined): boolean =>
	usage !== undefined &&
	((usage.providers?.length ?? 0) > 0 ||
		usage.todayCost !== undefined ||
		usage.todayTokens !== undefined ||
		usage.weekCost !== undefined ||
		usage.runningCost !== undefined);

export const visibleSections = (model: PopoverModel): PopoverSection[] => {
	const sections: PopoverSection[] = [];
	if (model.decisionsAvailable) sections.push('decisions');
	if (model.missed.length > 0) sections.push('missed');
	if (model.recent.length > 0) sections.push('switch');
	if (model.extras.today) sections.push('today');
	if ((model.extras.machines ?? []).length > 0) sections.push('machines');
	if (hasUsage(model.extras.usage)) sections.push('ai');
	if (model.pluginSections.length > 0) sections.push('plugins');
	return sections;
};

export const STALE_AFTER_S = 3600;

export const isStale = (bar: UsageBar): boolean =>
	bar.ageS !== undefined && bar.ageS > STALE_AFTER_S;

export const TODAY_CAP = 5;

type TodayExtras = NonNullable<PopoverExtras['today']>;

export const todayEntries = (
	routines: TodayExtras['routines'],
	deadlines: TodayExtras['deadlines'],
	cap = TODAY_CAP,
) => {
	const sorted = [...deadlines].sort((a, b) => a.dueAt - b.dueAt);
	const reserved = sorted.length > 0 && cap > 0 ? 1 : 0;
	const shownRoutines = routines.slice(0, cap - reserved);
	const shownDeadlines = sorted.slice(0, cap - shownRoutines.length);
	return {
		routines: shownRoutines,
		deadlines: shownDeadlines,
		hiddenRoutines: routines.length - shownRoutines.length,
	};
};

export const POPOVER_PADDING = 12;
export const FALLBACK_MAX_HEIGHT = 760;
export const BUSY_MS = 1500;

export const panelMaxHeight = (windowMax: number): number =>
	Math.max(windowMax - 2 * POPOVER_PADDING, 0);

export const markBusy = (busy: readonly string[], id: string): string[] =>
	busy.includes(id) ? [...busy] : [...busy, id];

export const usageBarPct = (bar: UsageBar): number =>
	Math.min(100, Math.max(0, Math.round(bar.pct)));
