import type { EventName, TrackArgs } from './catalog';
import type { Telemetry } from './client';

export type { EventName, EventProps, TrackArgs } from './catalog';

let instance: Telemetry | null = null;

export const setTelemetry = (next: Telemetry | null) => {
	instance = next;
};

export const getTelemetry = (): Telemetry | null => instance;

export const track = <N extends EventName>(name: N, ...args: TrackArgs<N>): void => {
	instance?.track(name, ...args);
};
