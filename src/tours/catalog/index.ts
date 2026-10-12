import type { TourDef, TourId } from '../types';
import { startTours } from './start';
import { tasksTours } from './tasks';
import { timeTours } from './time';

export const TOURS: TourDef[] = [...startTours, ...tasksTours, ...timeTours];

export function getTour(id: TourId): TourDef | undefined {
  return TOURS.find((tour) => tour.id === id);
}
