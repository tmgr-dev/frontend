/** Preserve input order while grouping once, instead of scanning for every calendar cell. */
export function indexEntriesByDate<T extends { date: string }>(
	entries: T[],
): Map<string, T[]> {
	const days = new Map<string, T[]>();
	for (const entry of entries) {
		const day = days.get(entry.date);
		if (day) day.push(entry);
		else days.set(entry.date, [entry]);
	}
	return days;
}
