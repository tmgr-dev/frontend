import type { TourDef, TourId } from '../types';
import { agentsTours } from './agents';
import { graphTours } from './graph';
import { pagesTours } from './pages';
import { personasTours } from './personas';
import { routinesTours } from './routines';
import { startTours } from './start';
import { tasksTours } from './tasks';
import { timeTours } from './time';
import { workspaceTours } from './workspace';

export const TOURS: TourDef[] = [
  ...startTours,
  ...tasksTours,
  ...timeTours,
  ...agentsTours,
  ...personasTours,
  ...workspaceTours,
  ...pagesTours,
  ...graphTours,
  ...routinesTours,
];

export function getTour(id: TourId): TourDef | undefined {
  return TOURS.find((tour) => tour.id === id);
}
