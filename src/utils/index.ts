import { clsx, type ClassValue } from 'clsx';
import { formatDistanceToNow } from 'date-fns';
import { twMerge } from 'tailwind-merge';

export function cn(...inputs: ClassValue[]) {
	return twMerge(clsx(inputs));
}

export function getTimeSinceUpdated(updatedAt: Date) {
	return formatDistanceToNow(updatedAt, { addSuffix: true });
}
