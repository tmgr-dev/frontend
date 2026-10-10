export const formatDuration = (seconds: number): string => {
	const hours = Math.floor(seconds / 3600);
	const minutes = Math.ceil((seconds % 3600) / 60);
	const hoursPart = hours > 0 ? `${hours} hour${hours === 1 ? '' : 's'} ` : '';
	return `${hoursPart}${minutes} minute${minutes === 1 ? '' : 's'}`;
};
