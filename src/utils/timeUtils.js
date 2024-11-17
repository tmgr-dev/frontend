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

// Если нужно сгруппировать все функции вместе
export const timeUtils = {
	secondsToHumanReadableString,
	secondsToCountdownObject,
	prepareClockNumber,
};
