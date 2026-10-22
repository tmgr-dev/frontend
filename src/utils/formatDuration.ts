export const formatDuration = (seconds: number | null | undefined): string => {
	const total = Number(seconds) || 0;
	const hours = Math.floor(total / 3600);
	const minutes = Math.ceil((total % 3600) / 60);
	const hoursPart = hours > 0 ? `${hours} hour${hours === 1 ? '' : 's'} ` : '';
	return `${hoursPart}${minutes} minute${minutes === 1 ? '' : 's'}`;
};
