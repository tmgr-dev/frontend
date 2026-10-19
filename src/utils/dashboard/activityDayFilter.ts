const MONTHS = [
	'Jan',
	'Feb',
	'Mar',
	'Apr',
	'May',
	'Jun',
	'Jul',
	'Aug',
	'Sep',
	'Oct',
	'Nov',
	'Dec',
];

export const toggleDay = (
	current: string | null | undefined,
	clicked: string,
): string | null => (current === clicked ? null : clicked);

// Split instead of new Date('YYYY-MM-DD'): that parses as UTC and shows the previous day west of UTC.
export const formatDayLabel = (date: string): string => {
	const [year, month, day] = date.split('-').map(Number);
	return `${MONTHS[month - 1]} ${day}, ${year}`;
};

export const activityCountLabel = (count: number): string =>
	`${count} ${count === 1 ? 'activity' : 'activities'}`;
