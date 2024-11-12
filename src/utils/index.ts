import { clsx, type ClassValue } from 'clsx';
import { twMerge } from 'tailwind-merge';
import dateFormat from 'dateformat';

export function cn(...inputs: ClassValue[]) {
	return twMerge(clsx(inputs));
}

export function getTimeSinceUpdated(updatedAt: Date) {
	const now = new Date();
	const diff = now.getTime() - updatedAt.getTime();
	const seconds = Math.floor(diff / 1000);
	const minutes = Math.floor(seconds / 60);
	const hours = Math.floor(minutes / 60);
	const days = Math.floor(hours / 24);
	const months = Math.floor(days / 30);
	const years = Math.floor(days / 365);

	if (years > 0) {
		return dateFormat(updatedAt, "yyyy 'years' ago");
	} else if (months > 0) {
		return dateFormat(updatedAt, "m 'months' ago");
	} else if (days > 0) {
		return dateFormat(updatedAt, "d 'days' ago");
	} else if (hours > 0) {
		return dateFormat(updatedAt, "h 'hours' ago");
	} else if (minutes > 0) {
		return dateFormat(updatedAt, "m 'minutes' ago");
	} else {
		return 'Just now';
	}
}
