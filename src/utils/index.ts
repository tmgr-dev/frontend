import { clsx, type ClassValue } from 'clsx';
import { twMerge } from 'tailwind-merge';
import { formatDistanceToNow } from 'date-fns';

export function cn(...inputs: ClassValue[]) {
	return twMerge(clsx(inputs));
}

export function getTimeSinceUpdated(updatedAt: Date) {
	return formatDistanceToNow(updatedAt, { addSuffix: true });
}
