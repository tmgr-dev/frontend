import type { DesktopPlatform } from './desktop';

export interface NavEntry {
	path: string;
	taskId: number | string | null;
}

export type NavDirection = 'back' | 'forward';

const same = (a: NavEntry, b: NavEntry): boolean =>
	a.path === b.path && String(a.taskId ?? '') === String(b.taskId ?? '');

export const createNavHistory = (cap = 100) => {
	let entries: NavEntry[] = [];
	let index = -1;
	let suppressed = false;

	const move = (delta: number): NavEntry | null => {
		const next = index + delta;
		if (next < 0 || next >= entries.length) return null;
		index = next;
		suppressed = true;
		return entries[index];
	};

	return {
		push(entry: NavEntry): void {
			if (suppressed) return;
			if (index >= 0 && same(entries[index], entry)) return;
			entries = entries.slice(0, index + 1);
			entries.push({ ...entry });
			if (entries.length > cap) entries = entries.slice(entries.length - cap);
			index = entries.length - 1;
		},
		replace(entry: NavEntry): void {
			if (suppressed) return;
			if (index < 0) return this.push(entry);
			entries[index] = { ...entry };
			if (index > 0 && same(entries[index - 1], entry)) {
				entries.splice(index, 1);
				index -= 1;
			}
		},
		back: () => move(-1),
		forward: () => move(1),
		release(): void {
			suppressed = false;
		},
		canBack: () => index > 0,
		canForward: () => index >= 0 && index < entries.length - 1,
		size: () => entries.length,
	};
};

export const navDirectionForKey = (
	event: Pick<
		KeyboardEvent,
		'key' | 'code' | 'metaKey' | 'altKey' | 'ctrlKey' | 'shiftKey'
	>,
	platform: DesktopPlatform | null,
): NavDirection | null => {
	if (event.shiftKey || event.ctrlKey) return null;
	if (platform !== 'windows' && platform !== 'linux') {
		if (!event.metaKey || event.altKey) return null;
		if (event.key === '[' || event.code === 'BracketLeft') return 'back';
		if (event.key === ']' || event.code === 'BracketRight') return 'forward';
		return null;
	}
	if (!event.altKey || event.metaKey) return null;
	if (event.key === 'ArrowLeft') return 'back';
	if (event.key === 'ArrowRight') return 'forward';
	return null;
};

export const navDirectionForMouse = (button: number): NavDirection | null =>
	button === 3 ? 'back' : button === 4 ? 'forward' : null;
