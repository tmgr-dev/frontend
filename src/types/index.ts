export interface Time {
	hours: number;
	minutes: number;
	seconds?: number;
}

export interface ExtendedTime extends Time {
	timeLeft: string;
}
