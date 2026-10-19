import { expiringSoon, type AllowedEntry } from '@/utils/agentApprovalPolicy';
import { isDesktopApp } from '@/utils/desktop';
import { invoke } from '@tauri-apps/api/core';
import { runnerAllowedList } from './api';

const STORAGE_KEY = 'tmgr:runner-allow-reminders';
const EVERY_MS = 6 * 3_600_000;

export const allowCommand = (code: string): string =>
	`tmgr runner allow ${code}`;

export const localDay = (now: number): string => {
	const d = new Date(now);
	return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(
		2,
		'0',
	)}-${String(d.getDate()).padStart(2, '0')}`;
};

export const dueReminders = (
	entries: AllowedEntry[],
	now: number,
	shown: Record<string, string>,
): AllowedEntry[] =>
	expiringSoon(entries, now).filter(
		(entry) => shown[entry.workspaceCode] !== localDay(now),
	);

const readShown = (): Record<string, string> => {
	try {
		const parsed = JSON.parse(localStorage.getItem(STORAGE_KEY) || '{}');
		return parsed && typeof parsed === 'object' ? parsed : {};
	} catch {
		return {};
	}
};

const writeShown = (shown: Record<string, string>): void => {
	try {
		localStorage.setItem(STORAGE_KEY, JSON.stringify(shown));
	} catch {
		return;
	}
};

export const checkAllowReminders = async (
	now: number = Date.now(),
): Promise<void> => {
	const { entries } = await runnerAllowedList();
	const shown = readShown();
	for (const entry of dueReminders(entries, now, shown)) {
		shown[entry.workspaceCode] = localDay(now);
		writeShown(shown);
		await invoke('plugin_notify', {
			title: 'Cloud agent runs expire soon',
			body: `Workspace ${
				entry.workspaceCode
			} stops accepting agent runs on this computer soon. Renew in a terminal: ${allowCommand(
				entry.workspaceCode,
			)}`,
			token: null,
		}).catch(() => undefined);
	}
};

let started = false;

export const startAllowReminders = (): void => {
	if (started || !isDesktopApp()) return;
	started = true;
	const run = () => void checkAllowReminders().catch(() => undefined);
	run();
	setInterval(run, EVERY_MS);
};
