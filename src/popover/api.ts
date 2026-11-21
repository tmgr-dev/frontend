import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { PopoverModel } from './types';

export const popoverState = () => invoke<PopoverModel>('popover_state');

export const popoverAction = (id: string) =>
	invoke<void>('popover_action', { id });

export const popoverQuickAdd = (title: string) =>
	invoke<void>('popover_quick_add', { title });

export const popoverResize = (height: number) =>
	invoke<void>('popover_resize', { height });

export const onPopoverModel = (
	handler: (model: PopoverModel) => void,
): Promise<UnlistenFn> =>
	listen<PopoverModel>('popover://model', (event) => handler(event.payload));

export const onPopoverShown = (handler: () => void): Promise<UnlistenFn> =>
	listen('popover://show', () => handler());
