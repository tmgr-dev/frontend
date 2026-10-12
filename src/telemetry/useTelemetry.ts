import { onScopeDispose, ref } from 'vue';
import type { TelemetrySnapshot } from './client';
import { onTelemetry } from './index';

export const useTelemetry = () => {
	const snapshot = ref<TelemetrySnapshot>({ active: false, installId: null, pending: 0 });
	let unsubscribe: (() => void) | null = null;
	const stopWaiting = onTelemetry((telemetry) => {
		unsubscribe?.();
		snapshot.value = telemetry.snapshot();
		unsubscribe = telemetry.subscribe(() => {
			snapshot.value = telemetry.snapshot();
		});
	});
	onScopeDispose(() => {
		stopWaiting();
		unsubscribe?.();
	});
	return { snapshot };
};

export const shortInstallId = (id: string | null): string =>
	id ? `${id.slice(0, 4)}…${id.slice(-4)}` : '';
