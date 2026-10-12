import { tourBus, type TourBus } from './events';

export function emitPageLinkAdded(kind: string, bus: TourBus = tourBus): void {
  bus.emit('page.link_added', { kind });
}

export function emitRoutineEditorOpened(isNew: boolean, bus: TourBus = tourBus): void {
  if (!isNew) bus.emit('routine.editor_opened');
}
