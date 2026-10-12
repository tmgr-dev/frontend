import type { TourBus } from './events';
import type { ToursApi } from './useTours';

const OFFER_RETRY_MS = 60_000;

export interface DesktopTriggerEnv {
  isDesktop(): boolean;
  windowLabel(): string | null;
}

export function installDesktopTriggers(
  bus: TourBus,
  tours: Pick<ToursApi, 'offer'>,
  env: DesktopTriggerEnv,
): () => void {
  if (!env.isDesktop() || env.windowLabel() !== 'main') return () => {};
  return bus.on('timer.started', () => {
    void tours.offer('desktop.menu-bar-timer', 'event', { retryMs: OFFER_RETRY_MS });
  });
}
