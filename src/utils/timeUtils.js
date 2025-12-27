export function formatTime(taskTime) {
	let hours = Math.floor(taskTime / 3600);
	let minutes = Math.ceil((taskTime % 3600) / 60);

	return `${
		hours > 0 ? hours + ' hour' + (hours > 1 ? 's' : '') : ''
	} ${minutes} minute${minutes > 1 ? 's' : ''}`;
}

/**
 * Converts seconds to "HH:MM" format
 */
export function secondsToHumanReadableString(seconds) {
	const { hours, minutes } = secondsToCountdownObject(seconds);
	return `${hours}:${minutes}`;
}

/**
 * Converts total seconds to an object with hours, minutes and seconds
 */
export function secondsToCountdownObject(seconds) {
	const second = seconds % 60;
	let minute = ((seconds - second) / 60) | 0;
	const hour = (minute / 60) | 0;
	minute = minute - hour * 60;

	return {
		hours: prepareClockNumber(hour),
		minutes: prepareClockNumber(minute),
		seconds: prepareClockNumber(second) || '00',
	};
}

/**
 * Adds leading zero to numbers less than 10
 */
export function prepareClockNumber(num) {
	return num < 10 ? '0' + num : num;
}

export function timeToSeconds(time) {
	if (!time) return 0;
	const [hours, minutes] = time.split(':').map(Number);
	return hours * 3600 + minutes * 60;
}

export function convertToHHMM(seconds) {
	const hours = Math.floor(seconds / 3600);
	const minutes = Math.floor((seconds % 3600) / 60);

	return `${hours.toString().padStart(2, '0')}:${minutes
		.toString()
		.padStart(2, '0')}`;
}

export function formatRelativeTime(dateString) {
	if (!dateString) return '';
	
	const date = new Date(dateString);
	const now = new Date();
	const diffMs = now - date;
	const diffSeconds = Math.floor(diffMs / 1000);
	const diffMinutes = Math.floor(diffSeconds / 60);
	const diffHours = Math.floor(diffMinutes / 60);
	const diffDays = Math.floor(diffHours / 24);

	if (diffMinutes < 1) {
		return 'just now';
	} else if (diffMinutes < 60) {
		return `${diffMinutes} minute${diffMinutes > 1 ? 's' : ''} ago`;
	} else if (diffHours < 24) {
		return `${diffHours} hour${diffHours > 1 ? 's' : ''} ago`;
	} else if (diffDays < 7) {
		return `${diffDays} day${diffDays > 1 ? 's' : ''} ago`;
	} else {
		const isCurrentYear = date.getFullYear() === now.getFullYear();
		if (!isCurrentYear) {
			const year = date.getFullYear();
			const month = String(date.getMonth() + 1).padStart(2, '0');
			const day = String(date.getDate()).padStart(2, '0');
			return `${year}/${month}/${day}`;
		}
		return date.toLocaleDateString('en-US', {
			month: 'short',
			day: 'numeric',
			hour: '2-digit',
			minute: '2-digit',
			hour12: false
		});
	}
}
