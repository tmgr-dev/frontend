export type PanelSide = 'left' | 'right';
export type WidthStorage = Pick<Storage, 'getItem' | 'setItem'>;

export const DEFAULT_WIDTH: Record<PanelSide, number> = {
	left: 300,
	right: 340,
};
export const MIN_WIDTH: Record<PanelSide, number> = { left: 220, right: 260 };
export const MAX_WIDTH: Record<PanelSide, number> = { left: 520, right: 560 };
export const MIN_CHAT_WIDTH = 400;
export const RIGHT_PANEL_HIDE_AT = 1100;

const storageKey = (side: PanelSide) => `manager.panel.${side}`;

export const clampWidth = (
	side: PanelSide,
	want: number,
	viewport: number,
	other: number,
): number => {
	const wanted = Number.isFinite(want) ? want : DEFAULT_WIDTH[side];
	const room = Number.isFinite(viewport)
		? viewport - other - MIN_CHAT_WIDTH
		: MAX_WIDTH[side];
	const max = Math.max(MIN_WIDTH[side], Math.min(MAX_WIDTH[side], room));
	return Math.round(Math.min(max, Math.max(MIN_WIDTH[side], wanted)));
};

const defaultStorage = (): WidthStorage | null => {
	try {
		return localStorage;
	} catch {
		return null;
	}
};

export const loadWidth = (
	side: PanelSide,
	storage: WidthStorage | null = defaultStorage(),
): number => {
	try {
		const raw = storage?.getItem(storageKey(side));
		const n = raw === null || raw === undefined ? NaN : Number(raw);
		return Number.isFinite(n)
			? clampWidth(side, n, Infinity, 0)
			: DEFAULT_WIDTH[side];
	} catch {
		return DEFAULT_WIDTH[side];
	}
};

export const saveWidth = (
	side: PanelSide,
	width: number,
	storage: WidthStorage | null = defaultStorage(),
): void => {
	try {
		storage?.setItem(storageKey(side), String(Math.round(width)));
	} catch {
		return;
	}
};

export const shellColumns = (
	left: number,
	right: number,
	viewport: number,
): string =>
	viewport <= RIGHT_PANEL_HIDE_AT
		? `${left}px minmax(0, 1fr)`
		: `${left}px minmax(0, 1fr) ${right}px`;
