export const BOTTOM_THRESHOLD = 48;
export const TOP_THRESHOLD = 40;

export interface ScrollMetrics {
	scrollTop: number;
	scrollHeight: number;
	clientHeight: number;
}

export const isNearBottom = (
	metrics: ScrollMetrics,
	threshold = BOTTOM_THRESHOLD,
): boolean =>
	metrics.scrollHeight - metrics.scrollTop - metrics.clientHeight <= threshold;

export const shouldLoadOlder = (
	scrollTop: number,
	busy: boolean,
	done: boolean,
): boolean => scrollTop < TOP_THRESHOLD && !busy && !done;

export const arrivalsAfter = <T extends { id: string }>(
	lastId: string | undefined,
	list: readonly T[],
): T[] => {
	const index =
		lastId === undefined ? -1 : list.findIndex((m) => m.id === lastId);
	return list.slice(index + 1);
};

export const newBelowCount = (
	current: number,
	arrived: number,
	pinned: boolean,
): number => (pinned ? 0 : current + arrived);

export const anchoredScrollTop = (
	scrollTop: number,
	prevHeight: number,
	nextHeight: number,
): number => scrollTop + (nextHeight - prevHeight);
