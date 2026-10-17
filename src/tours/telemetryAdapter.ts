import type { EventName } from '@/telemetry';
import { track } from '@/telemetry';
import { setTourTelemetry } from './telemetry';

export const installTourTelemetry = (): void => {
	setTourTelemetry({
		track: (name, props) => track(name as EventName, props as never),
	});
};
