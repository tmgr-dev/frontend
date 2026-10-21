export const PRIORITY_LEVELS = ['urgent', 'high', 'medium', 'low'] as const;

export type PriorityLevel = (typeof PRIORITY_LEVELS)[number];

export const PRIORITY_FILTER_OPTIONS: {
	value: PriorityLevel;
	label: string;
}[] = [
	{ value: 'urgent', label: 'Urgent' },
	{ value: 'high', label: 'High' },
	{ value: 'medium', label: 'Medium' },
	{ value: 'low', label: 'Low' },
];

const DEFAULT_PRIORITY: PriorityLevel = 'medium';

function toLevel(value: unknown): PriorityLevel | null {
	const level = typeof value === 'string' ? value.trim().toLowerCase() : '';
	return (PRIORITY_LEVELS as readonly string[]).includes(level)
		? (level as PriorityLevel)
		: null;
}

export function taskPriority(task: { priority?: unknown }): PriorityLevel {
	return toLevel(task.priority) ?? DEFAULT_PRIORITY;
}

export function priorityRank(task: { priority?: unknown }): number {
	return PRIORITY_LEVELS.indexOf(taskPriority(task));
}

export function parsePriorityFilter(value: unknown): PriorityLevel[] {
	const parts = (Array.isArray(value) ? value : [value])
		.filter((part): part is string => typeof part === 'string')
		.flatMap((part) => part.split(','));
	const levels = new Set(parts.map(toLevel));
	return PRIORITY_LEVELS.filter((level) => levels.has(level));
}

export function serializePriorityFilter(levels: readonly string[]): string {
	return parsePriorityFilter(levels.join(',')).join(',');
}

export function matchesPriorityFilter(
	task: { priority?: unknown },
	levels: readonly string[],
): boolean {
	return !levels.length || levels.includes(taskPriority(task));
}

export function sortByPriority<T extends { priority?: unknown }>(
	tasks: readonly T[],
): T[] {
	return [...tasks].sort((a, b) => priorityRank(a) - priorityRank(b));
}
