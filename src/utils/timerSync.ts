/**
 * Cross-tab timer state (TM-224). The API broadcasts `task-countdown-started` /
 * `task-countdown-stopped` on the user's own channel, which is what makes the same user's other
 * tabs able to follow a timer they did not start themselves. These helpers decide whether an
 * incoming task actually moves the timer, so a re-render only happens when it did.
 */

export interface TimerState {
	start_time?: number | null;
	common_time?: number | null;
	[key: string]: unknown;
}

const asNumber = (value: unknown): number | null =>
	value === null || value === undefined || value === '' ? null : Number(value);

/** True when the incoming task shows a different running-timer state than the form holds. */
export function timerStateDiffers(current: TimerState, incoming: TimerState): boolean {
	return (
		asNumber(current.start_time) !== asNumber(incoming.start_time) ||
		asNumber(current.common_time) !== asNumber(incoming.common_time)
	);
}

/**
 * Copies the timer of `incoming` onto `form` when it belongs to the same task and actually moved.
 * Returns whether anything changed, so callers can skip work when it did not.
 */
export function applyTimerState(
	form: TimerState & { id?: number | null },
	incoming: (TimerState & { id?: number | null }) | null | undefined,
): boolean {
	if (!incoming || !form.id || Number(incoming.id) !== Number(form.id)) return false;
	if (!timerStateDiffers(form, incoming)) return false;

	form.start_time = asNumber(incoming.start_time);
	form.common_time = asNumber(incoming.common_time);
	return true;
}
