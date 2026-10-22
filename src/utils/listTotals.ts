export const formatTaskCount = (count: number | null | undefined): string => {
	const total = Number(count) || 0;
	return `${total} task${total === 1 ? '' : 's'}`;
};

export const sumTaskSeconds = (
	tasks: { common_time?: number | null }[],
): number =>
	tasks.reduce((sum, task) => sum + (Number(task.common_time) || 0), 0);

// The server total covers every matching task but is fixed at load time; realtime changes
// (timer stops, inserts, removals) only reach the loaded page, so the page's drift since
// the load is added on top. Without a server total the page sum is the total.
export const listTotalSeconds = (
	serverTotal: number | string | null | undefined,
	pageSeconds: number,
	loadedPageSeconds: number,
): number =>
	serverTotal
		? Math.max(0, (Number(serverTotal) || 0) + pageSeconds - loadedPageSeconds)
		: pageSeconds;
