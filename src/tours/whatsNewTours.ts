import { DESKTOP_RELEASE_TOURS } from './catalog/desktop';

export type TourStartStatus = 'available' | 'completed' | 'unavailable';

export const RELEASE_TOURS: Record<string, string[]> = DESKTOP_RELEASE_TOURS;

export const WHATS_NEW_TOUR_ID = 'desktop.whats-new';

export function availableTours(
	version: string,
	startStatus: (id: string) => TourStartStatus,
	releaseTours: Record<string, string[]> = RELEASE_TOURS,
): string[] {
	return (releaseTours[version] ?? []).filter(
		(id) => startStatus(id) === 'available',
	);
}
