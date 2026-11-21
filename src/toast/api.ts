import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { ToastList } from './types';

export const toastState = () => invoke<ToastList>('toast_state');

export const toastAction = (toastId: string, actionId: string) =>
	invoke<void>('toast_action', { toastId, actionId });

export const toastDismiss = (id: string) =>
	invoke<void>('toast_dismiss', { id });

export const toastHover = (id: string, hovering: boolean) =>
	invoke<void>('toast_hover', { id, hovering });

export const toastResize = (height: number) =>
	invoke<void>('toast_resize', { height });

export const toastExpand = () => invoke<void>('toast_expand');

export const onToastList = (
	handler: (list: ToastList) => void,
): Promise<UnlistenFn> =>
	listen<ToastList>('toast://list', (event) => handler(event.payload));
