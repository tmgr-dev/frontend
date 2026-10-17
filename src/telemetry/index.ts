import type { EventName, TrackArgs } from './catalog';
import type { Telemetry } from './client';

export type { EventName, EventProps, TrackArgs } from './catalog';

let instance: Telemetry | null = null;

const waiting = new Set<(telemetry: Telemetry) => void>();

export const setTelemetry = (next: Telemetry | null) => {
	instance = next;
	if (next) waiting.forEach((fn) => fn(next));
};

export const onTelemetry = (
	fn: (telemetry: Telemetry) => void,
): (() => void) => {
	if (instance) fn(instance);
	waiting.add(fn);
	return () => waiting.delete(fn);
};

export const getTelemetry = (): Telemetry | null => instance;

export const track = <N extends EventName>(
	name: N,
	...args: TrackArgs<N>
): void => {
	instance?.track(name, ...args);
};
