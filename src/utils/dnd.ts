import { reactive } from 'vue';

export type DndOption = 'off' | '1h' | '3h' | 'tomorrow';

interface DndStored {
	option: DndOption;
	until: number | null;
}

const KEY = 'app.dnd';

const read = (): DndStored => {
	try {
		const raw = localStorage.getItem(KEY);
		if (!raw) return { option: 'off', until: null };
		const parsed = JSON.parse(raw);
		return {
			option: ['off', '1h', '3h', 'tomorrow'].includes(parsed?.option)
				? parsed.option
				: 'off',
			until: typeof parsed?.until === 'number' ? parsed.until : null,
		};
	} catch {
		return { option: 'off', until: null };
	}
};

export const dndState = reactive<DndStored>(read());

/** Bumped on an interval so computed values that read `isDndActive()` re-evaluate as a timer expires. */
export const dndClock = reactive({ now: Date.now() });

const persist = () => {
	try {
		localStorage.setItem(
			KEY,
			JSON.stringify({ option: dndState.option, until: dndState.until }),
		);
	} catch {
		// Do not disturb just won't survive a restart.
	}
};

export const isDndActive = (now: number = Date.now()): boolean =>
	dndState.option !== 'off' && dndState.until != null && now < dndState.until;

/** Before 9am, "until tomorrow" means today at 9am: that hour has not happened yet today. */
const tomorrowNine = (from: number): number => {
	const d = new Date(from);
	if (d.getHours() < 9) {
		d.setHours(9, 0, 0, 0);
		return d.getTime();
	}
	d.setDate(d.getDate() + 1);
	d.setHours(9, 0, 0, 0);
	return d.getTime();
};

const untilFor = (option: DndOption, from: number): number | null => {
	switch (option) {
		case '1h':
			return from + 60 * 60_000;
		case '3h':
			return from + 3 * 60 * 60_000;
		case 'tomorrow':
			return tomorrowNine(from);
		default:
			return null;
	}
};

const pushToTray = async (option: DndOption) => {
	try {
		const { invoke } = await import('@tauri-apps/api/core');
		await invoke('dnd_update', { option });
	} catch (error) {
		console.error('dnd_update failed', error);
	}
};

export const setDnd = (option: DndOption): void => {
	dndState.option = option;
	dndState.until = untilFor(option, Date.now());
	persist();
	void pushToTray(option);
};

/** Turns an expired "until" off so the tray checkmark and status bar do not stay on past it. */
export const expireDndIfNeeded = (now: number = Date.now()): void => {
	if (dndState.option === 'off' || dndState.until == null || now < dndState.until) return;
	dndState.option = 'off';
	dndState.until = null;
	persist();
	void pushToTray('off');
};

/** Called once at startup: an "until" saved from a previous session may already have expired. */
export const syncDndToTray = (): void => {
	expireDndIfNeeded();
	void pushToTray(isDndActive() ? dndState.option : 'off');
};

let ticker: ReturnType<typeof setInterval> | null = null;

/** Keeps `dndClock` moving so an "until" time expiring is reflected in the UI without a page reload. */
export const startDndClock = (): void => {
	if (ticker) return;
	ticker = setInterval(() => {
		dndClock.now = Date.now();
		expireDndIfNeeded(dndClock.now);
	}, 30_000);
};
