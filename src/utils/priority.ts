import type { TaskPriority } from '@/types/dashboard';
import {
	ArrowDownIcon,
	ArrowRightIcon,
	ArrowUpIcon,
	ExclamationTriangleIcon,
} from '@heroicons/vue/24/outline';

export const PRIORITIES = [
	{
		value: 'urgent',
		label: 'Urgent',
		icon: ExclamationTriangleIcon,
		colorClass: 'text-red-500 dark:text-red-400',
	},
	{
		value: 'high',
		label: 'High',
		icon: ArrowUpIcon,
		colorClass: 'text-orange-500 dark:text-orange-400',
	},
	{
		value: 'medium',
		label: 'Medium',
		icon: ArrowRightIcon,
		colorClass: 'text-yellow-500 dark:text-yellow-400',
	},
	{
		value: 'low',
		label: 'Low',
		icon: ArrowDownIcon,
		colorClass: 'text-green-500 dark:text-green-400',
	},
] as const satisfies ReadonlyArray<{
	value: TaskPriority;
	label: string;
	icon: unknown;
	colorClass: string;
}>;

export type PriorityOption = (typeof PRIORITIES)[number];

export const normalizePriority = (value: unknown): TaskPriority =>
	PRIORITIES.some((p) => p.value === value)
		? (value as TaskPriority)
		: 'medium';

export const priorityOption = (value: unknown): PriorityOption =>
	PRIORITIES.find(
		(p) => p.value === normalizePriority(value),
	) as PriorityOption;

export const showPriorityBadge = (value: unknown): boolean =>
	normalizePriority(value) !== 'medium';

export const priorityRank = (task: { priority?: unknown }): number =>
	PRIORITIES.findIndex((p) => p.value === normalizePriority(task.priority));

export const parsePriorityFilter = (value: unknown): TaskPriority[] => {
	const parts = new Set(
		(Array.isArray(value) ? value : [value])
			.filter((part): part is string => typeof part === 'string')
			.flatMap((part) => part.split(','))
			.map((part) => part.trim().toLowerCase()),
	);
	return PRIORITIES.map((p) => p.value).filter((level) => parts.has(level));
};

export const serializePriorityFilter = (levels: readonly string[]): string =>
	parsePriorityFilter(levels.join(',')).join(',');

export const matchesPriorityFilter = (
	task: { priority?: unknown },
	levels: readonly string[],
): boolean =>
	!levels.length || levels.includes(normalizePriority(task.priority));

export const sortByPriority = <T extends { priority?: unknown }>(
	tasks: readonly T[],
): T[] => [...tasks].sort((a, b) => priorityRank(a) - priorityRank(b));
