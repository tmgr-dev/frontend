import { loadResetSeen, saveResetSeen, unseenResetAt } from '../historyReset';

const alert = (kind: string, at_ms: number) => ({ kind, device_id: '', at_ms });

describe('history reset notice', () => {
	it('shows the newest reset until it was seen', () => {
		const alerts = [
			alert('room_conflict', 50),
			alert('history_reset', 10),
			alert('history_reset', 20),
		];
		expect(unseenResetAt(alerts, 0)).toBe(20);
		expect(unseenResetAt(alerts, 20)).toBe(0);
		expect(unseenResetAt([alert('room_conflict', 50)], 0)).toBe(0);
	});

	it('remembers the dismissed reset and survives a broken storage', () => {
		const map = new Map<string, string>();
		const storage = {
			getItem: (k: string) => map.get(k) ?? null,
			setItem: (k: string, v: string) => void map.set(k, v),
		};
		expect(loadResetSeen(storage)).toBe(0);
		saveResetSeen(20, storage);
		expect(loadResetSeen(storage)).toBe(20);
		const broken = {
			getItem: () => {
				throw new Error('denied');
			},
			setItem: () => {
				throw new Error('denied');
			},
		};
		expect(loadResetSeen(broken)).toBe(0);
		expect(() => saveResetSeen(1, broken)).not.toThrow();
		expect(loadResetSeen(null)).toBe(0);
	});
});
