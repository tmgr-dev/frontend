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
