const sortFields = new Set([
	'updated_at',
	'created_at',
	'expired_at',
	'scheduled_date',
]);

function positiveInteger(value: unknown, fallback: number): number {
	if (typeof value !== 'string' && typeof value !== 'number') return fallback;
	const parsed = Number(value);
	return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : fallback;
}

export function readTaskListQuery(query: Record<string, unknown>) {
	return {
		page: positiveInteger(query.page, 1),
		perPage: positiveInteger(query.per_page, 10),
		sort:
			typeof query.sort === 'string' && sortFields.has(query.sort)
				? query.sort
				: 'updated_at',
		direction: query.direction === 'asc' ? ('asc' as const) : ('desc' as const),
	};
}
