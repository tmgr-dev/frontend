export type TourEventName =
	| 'tour_offered'
	| 'tour_started'
	| 'tour_step_viewed'
	| 'tour_step_completed'
	| 'tour_step_skipped'
	| 'tour_step_unavailable'
	| 'tour_paused'
	| 'tour_completed'
	| 'tour_skipped'
	| 'tips_mode_changed';

export interface TourTelemetry {
	track(name: TourEventName, props: Record<string, string | number>): void;
}

const noopTelemetry: TourTelemetry = { track() {} };

let current: TourTelemetry = noopTelemetry;

export function setTourTelemetry(t: TourTelemetry): void {
	current = t;
}

export function tourTelemetry(): TourTelemetry {
	return current;
}
