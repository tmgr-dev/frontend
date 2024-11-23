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
