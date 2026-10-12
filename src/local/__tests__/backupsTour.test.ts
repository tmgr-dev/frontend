import { tourBus } from '@/tours/events';
import { invoke } from '@tauri-apps/api/core';
import { backupNow } from '../backups';

jest.mock('@tauri-apps/api/core', () => ({ invoke: jest.fn() }));

describe('backupNow tour event', () => {
	it('emits backup.created with the kind on success', async () => {
		(invoke as jest.Mock).mockResolvedValue({ kind: 'manual' });
		const seen: unknown[] = [];
		const off = tourBus.on('backup.created', (p) => seen.push(p));
		await backupNow('local-personal');
		off();
		expect(seen).toEqual([{ kind: 'manual' }]);
	});

	it('emits nothing when the backup fails', async () => {
		(invoke as jest.Mock).mockRejectedValue(new Error('disk full'));
		const seen: unknown[] = [];
		const off = tourBus.on('backup.created', (p) => seen.push(p));
		await expect(backupNow('local-personal')).rejects.toThrow('disk full');
		off();
		expect(seen).toEqual([]);
	});
});
