import { setTelemetry } from '@/telemetry';
import { tourTelemetry } from '../telemetry';
import { installTourTelemetry } from '../telemetryAdapter';

describe('installTourTelemetry', () => {
	afterEach(() => setTelemetry(null));

	it('forwards tour events to the telemetry client', () => {
		const track = jest.fn();
		setTelemetry({ track } as never);
		installTourTelemetry();
		tourTelemetry().track('tour_started', { tour: 'start.nav', trigger: 'hub' });
		expect(track).toHaveBeenCalledWith('tour_started', { tour: 'start.nav', trigger: 'hub' });
	});

	it('is a no-op while no client exists', () => {
		setTelemetry(null);
		installTourTelemetry();
		expect(() => tourTelemetry().track('tour_started', { tour: 'start.nav', trigger: 'hub' })).not.toThrow();
	});
});
