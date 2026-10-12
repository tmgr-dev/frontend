import { tourBus } from '@/tours/events';

export interface BackupInfo {
	file: string;
	path: string;
	size: number;
	created_at: string;
	kind: 'auto' | 'manual' | 'migration';
}

export interface BackupStatus {
	code: string;
	name: string;
	backups_dir: string;
	last_backup_at: string | null;
	last_error: string | null;
}

const invoke = async <T>(
	command: string,
	args?: Record<string, unknown>,
): Promise<T> => {
	const core = await import('@tauri-apps/api/core');
	return core.invoke<T>(command, args);
};

export const backupStatus = (): Promise<BackupStatus[]> =>
	invoke<BackupStatus[]>('local_backup_status');

export const backupNow = async (code: string): Promise<BackupInfo> => {
	const info = await invoke<BackupInfo>('local_backup_now', { code });
	tourBus.emit('backup.created', { kind: info?.kind ?? 'manual' });
	return info;
};

export const listBackups = (code: string): Promise<BackupInfo[]> =>
	invoke<BackupInfo[]>('local_backup_list', { code });

export const revealBackups = (code: string): Promise<void> =>
	invoke<void>('local_backup_reveal', { code });

export const revealWorkspaceFolder = (code: string): Promise<void> =>
	invoke<void>('local_reveal', { code, relative: null });
