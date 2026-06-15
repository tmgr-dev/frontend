/**
 * Returns a function that coalesces concurrent calls into a single in-flight
 * promise. While a worker is running, all callers share its result; once it
 * settles (resolve OR reject) the gate reopens for the next call.
 */
export function singleFlight<T>() {
	let inFlight: Promise<T> | null = null;
	return (worker: () => Promise<T>): Promise<T> => {
		if (inFlight) return inFlight;
		inFlight = worker().finally(() => {
			inFlight = null;
		});
		return inFlight;
	};
}
