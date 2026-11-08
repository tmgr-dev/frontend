import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { BeginPairingArgs, PairingView, PhoneView } from './types';

export const phonesList = () => invoke<PhoneView[]>('phones_list');

export const phonesPairing = () => invoke<PairingView | null>('phones_pairing');

export const phonesConfirm = (code: string) =>
	invoke<void>('phones_confirm', { code });

export const phonesCancel = () => invoke<void>('phones_cancel');

export const phonesRevoke = (id: string) =>
	invoke<void>('phones_revoke', { id });

export const phonesOpenWindow = () => invoke<void>('phones_open_window');

export const phonesBeginPairing = (args: BeginPairingArgs) =>
	invoke<void>('phones_begin_pairing', { ...args });

export const onPhonesChanged = (handler: () => void): Promise<UnlistenFn> =>
	listen('phones://changed', () => handler());
