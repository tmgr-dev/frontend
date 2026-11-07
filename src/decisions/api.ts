import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { Answer, CardView, DiffGroups, OutgoingMessage } from './types';

export const decisionsList = () => invoke<CardView[]>('decisions_list');

export const decisionsGet = (id: string) =>
	invoke<CardView>('decisions_get', { id });

export const decisionsAnswer = (id: string, answer: Answer) =>
	invoke<void>('decisions_answer', { id, answer });

export const decisionsMessage = (id: string, message: OutgoingMessage) =>
	invoke<void>('decisions_message', { id, message });

export const decisionsDiff = (id: string) =>
	invoke<DiffGroups>('decisions_diff', { id });

export const decisionsOpenLink = (url: string) =>
	invoke<void>('decisions_open_link', { url });

export const onDecisionsChanged = (handler: () => void): Promise<UnlistenFn> =>
	listen('decisions://changed', () => handler());
