import { describeAccelerator, mergeShortcuts } from '@/utils/desktopShortcuts';
import type { OnboardingApi } from '../api';
import type { ToursHostDeps } from '../engine';
import { createTours } from '../engine';
import { createTourBus } from '../events';
import { createLease } from '../lease';
import { setTourTelemetry } from '../telemetry';
import { COOLDOWN_KEY } from '../triggers';
import type { Platform } from '../types';

const memory = () => {
	const data = new Map<string, string>();
	return {
		getItem: (k: string) => data.get(k) ?? null,
		setItem: (k: string, v: string) => void data.set(k, v),
		removeItem: (k: string) => void data.delete(k),
	};
};
const flush = () => new Promise((r) => setImmediate(r));

const make = (over: Partial<ToursHostDeps> = {}, tipsMode = 'auto') => {
	const api: jest.Mocked<OnboardingApi> = {
		fetchState: jest
			.fn()
			.mockResolvedValue({ tips_mode: tipsMode, tours_generation: 1 }),
		putTour: jest.fn().mockResolvedValue({}),
		putPreferences: jest.fn().mockResolvedValue({}),
		resetTours: jest.fn().mockResolvedValue({ tours_generation: 2 }),
		setIntent: jest.fn().mockResolvedValue({}),
		fetchFacts: jest.fn().mockResolvedValue({}),
		removeStarterTasks: jest.fn().mockResolvedValue({ removed: 0 }),
	};
	const track = jest.fn();
	setTourTelemetry({ track });
	const adapter = { show: jest.fn(), refresh: jest.fn(), hide: jest.fn() };
	const timers: Array<{ fn: () => void; at: number; live: boolean }> = [];
	const clock = { now: 1_000_000, busy: false };
	const storage = memory();
	const deps: ToursHostDeps = {
		api,
		bus: createTourBus(),
		lease: createLease({
			storage: memory(),
			now: () => clock.now,
			ownerId: 'a',
		}),
		storage,
		session: memory(),
		now: () => clock.now,
		platform: () => 'desktop' as Platform,
		mobile: () => false,
		account: () => 'cloud',
		isOwner: () => true,
		modulesKnown: () => true,
		isModuleEnabled: () => true,
		currentRouteName: () => 'WorkspaceTasksList',
		navigate: () => {},
		isBusy: () => clock.busy,
		overlay: () => ({ adapter }),
		setTimer: (fn, ms) => {
			const timer = { fn, at: clock.now + ms, live: true };
			timers.push(timer);
			return () => {
				timer.live = false;
			};
		},
		resolveAnchor: async () => ({ id: 'x' }) as unknown as Element,
		...over,
	};
	const engine = createTours(deps);
	const advance = async (ms: number) => {
		clock.now += ms;
		for (const timer of timers.filter((t) => t.live && t.at <= clock.now)) {
			timer.live = false;
			timer.fn();
		}
		await flush();
	};
	return { engine, api, adapter, track, clock, advance, storage, deps };
};

describe('requires.capability', () => {
	const item = (h: ReturnType<typeof make>) =>
		h.engine.api.hubItems.value.find(
			(i) => i.tour.id === 'desktop.local-agent',
		)!;

	it('marks the tour unavailable (platform) when the host lacks the capability', () => {
		const h = make({ capability: () => false });
		expect(item(h)).toMatchObject({
			available: false,
			unavailableReason: 'platform',
		});
	});

	it('is available when the capability is supported', () => {
		const h = make({ capability: (name) => name === 'localAccess' });
		expect(item(h).available).toBe(true);
	});

	it('refuses to start when unsupported', async () => {
		const h = make({ capability: () => false });
		await h.engine.init();
		await h.engine.api.start('desktop.local-agent', 'hub');
		expect(h.engine.api.active.value).toBeNull();
	});
});

describe('offer', () => {
	it('starts an eligible tour and counts as the session auto tour', async () => {
		const h = make();
		await h.engine.init();
		expect(await h.engine.api.offer('desktop.menu-bar-timer', 'event')).toBe(
			true,
		);
		expect(h.engine.api.active.value?.tour.id).toBe('desktop.menu-bar-timer');
		expect(h.track).toHaveBeenCalledWith('tour_offered', {
			tour: 'desktop.menu-bar-timer',
			trigger: 'event',
		});
		h.engine.api.skipTour();
		expect(await h.engine.api.offer('desktop.backups', 'event')).toBe(false);
	});

	it('respects tips_mode', async () => {
		for (const mode of ['hub_only', 'off']) {
			const h = make({}, mode);
			await h.engine.init();
			expect(await h.engine.api.offer('desktop.menu-bar-timer', 'event')).toBe(
				false,
			);
			expect(h.engine.api.active.value).toBeNull();
		}
	});

	it('respects the cooldown', async () => {
		const h = make();
		await h.engine.init();
		h.storage.setItem(COOLDOWN_KEY, JSON.stringify(h.clock.now - 1000));
		expect(await h.engine.api.offer('desktop.menu-bar-timer', 'event')).toBe(
			false,
		);
	});

	it('does not offer a tour that already has state', async () => {
		const h = make();
		await h.engine.init();
		await h.engine.api.start('desktop.menu-bar-timer', 'hub');
		h.engine.api.skipStep();
		h.engine.api.skipStep();
		await flush();
		expect(
			await h.engine.api.offer('desktop.menu-bar-timer', 'event', {
				retryMs: 60_000,
			}),
		).toBe(false);
	});

	it('returns false at once when busy and no retry is asked for', async () => {
		const h = make();
		await h.engine.init();
		h.clock.busy = true;
		expect(await h.engine.api.offer('desktop.menu-bar-timer', 'event')).toBe(
			false,
		);
	});

	it('retries while busy and starts once the dialog closes', async () => {
		const h = make({ currentRouteName: () => null });
		await h.engine.init();
		h.clock.busy = true;
		const result = h.engine.api.offer('desktop.menu-bar-timer', 'event', {
			retryMs: 60_000,
		});
		await advance(h, 10_000);
		expect(h.engine.api.active.value).toBeNull();
		h.clock.busy = false;
		await advance(h, 2_000);
		expect(await result).toBe(true);
		expect(h.engine.api.active.value?.tour.id).toBe('desktop.menu-bar-timer');
	});

	it('gives up after the retry window', async () => {
		const h = make();
		await h.engine.init();
		h.clock.busy = true;
		const result = h.engine.api.offer('desktop.menu-bar-timer', 'event', {
			retryMs: 6_000,
		});
		await advance(h, 2_000);
		await advance(h, 2_000);
		await advance(h, 2_000);
		expect(await result).toBe(false);
		expect(h.engine.api.active.value).toBeNull();
	});

	it('shares one pending offer per tour', async () => {
		const h = make();
		await h.engine.init();
		h.clock.busy = true;
		const a = h.engine.api.offer('desktop.menu-bar-timer', 'event', {
			retryMs: 60_000,
		});
		const b = h.engine.api.offer('desktop.menu-bar-timer', 'event', {
			retryMs: 60_000,
		});
		expect(a).toBe(b);
		h.engine.dispose();
		expect(await a).toBe(false);
	});

	it('is false for an unknown tour or an unavailable one', async () => {
		const h = make({ platform: () => 'web' });
		await h.engine.init();
		expect(await h.engine.api.offer('nope', 'event')).toBe(false);
		expect(await h.engine.api.offer('desktop.menu-bar-timer', 'event')).toBe(
			false,
		);
	});
});

const advance = (h: ReturnType<typeof make>, ms: number) => h.advance(ms);

describe('body parts', () => {
	const keycaps = (platform: 'macos' | 'windows') => {
		const config = mergeShortcuts({});
		return () => ({
			mac: platform === 'macos',
			tray: platform === 'macos' ? 'menu bar' : 'system tray',
			shortcut: (a: string) => config[a as 'quickAdd'],
			describe: (acc: string) => describeAccelerator(acc, platform),
		});
	};

	it('resolves key tokens for the active step', async () => {
		const h = make({ keycaps: keycaps('windows') });
		await h.engine.init();
		await h.engine.api.start('desktop.quick-add', 'hub');
		expect(h.engine.api.active.value?.bodyParts).toEqual([
			{ kind: 'text', text: 'Press ' },
			{ kind: 'key', text: 'Alt+Space' },
			{ kind: 'text', text: ' in any app. A small capture window opens.' },
		]);
	});

	it('uses the platform body variant', async () => {
		const h = make({ keycaps: keycaps('macos') });
		await h.engine.init();
		await h.engine.api.start('desktop.menu-bar-timer', 'hub');
		expect(h.engine.api.active.value?.bodyParts?.[0].text).toContain(
			'menu bar',
		);
	});

	it('leaves plain steps untouched', async () => {
		const h = make({ platform: () => 'desktop_local', account: () => 'local' });
		await h.engine.init();
		await h.engine.api.start('desktop.local-workspace', 'hub');
		expect(h.engine.api.active.value?.bodyParts).toBeUndefined();
	});
});

describe('overlay blocking', () => {
	const last = (h: ReturnType<typeof make>) =>
		h.adapter.show.mock.calls[h.adapter.show.mock.calls.length - 1][0];

	it('a passive wait step with no anchor does not block the page', async () => {
		const h = make();
		await h.engine.init();
		await h.engine.api.start('desktop.quick-add', 'hub');
		expect(last(h)).toMatchObject({ blocking: true });
		h.engine.api.next();
		await flush();
		expect(h.engine.api.active.value?.step.type).toBe('wait');
		expect(last(h)).toMatchObject({
			mode: 'passive',
			anchorEl: null,
			blocking: false,
		});
	});

	it('a modal step with an anchor still blocks', async () => {
		const h = make();
		await h.engine.init();
		await h.engine.api.start('start.nav', 'hub');
		expect(last(h)).toMatchObject({ mode: 'modal', blocking: true });
		expect(last(h).anchorEl).toBeTruthy();
	});
});
