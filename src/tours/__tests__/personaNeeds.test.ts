import { createTours } from '../engine';
import type { ToursHostDeps } from '../engine';
import { createTourBus } from '../events';
import { createLease } from '../lease';
import { setTourTelemetry } from '../telemetry';
import { buildHubModel } from '../hubModel';
import { TOURS } from '../catalog';

const memory = () => {
	const data = new Map<string, string>();
	return {
		getItem: (k: string) => data.get(k) ?? null,
		setItem: (k: string, v: string) => void data.set(k, v),
		removeItem: (k: string) => void data.delete(k),
	};
};
const flush = () => new Promise((r) => setImmediate(r));

const make = (
	hasPersonas: () => Promise<boolean>,
	gate: { known: boolean; enabled: boolean } = { known: true, enabled: true },
) => {
	setTourTelemetry({ track: jest.fn() });
	const bus = createTourBus();
	const deps: ToursHostDeps = {
		api: {
			fetchState: jest.fn().mockResolvedValue({ tips_mode: 'hub_only', tours_generation: 1 }),
			putTour: jest.fn().mockResolvedValue({}),
			putPreferences: jest.fn().mockResolvedValue({}),
			resetTours: jest.fn().mockResolvedValue({}),
			setIntent: jest.fn().mockResolvedValue({}),
			fetchFacts: jest.fn().mockResolvedValue({}),
			removeStarterTasks: jest.fn().mockResolvedValue({}),
		} as never,
		bus,
		lease: createLease({ storage: memory(), now: () => Date.now(), ownerId: 'a' }),
		storage: memory(),
		session: memory(),
		now: () => Date.now(),
		platform: () => 'web',
		mobile: () => false,
		account: () => 'cloud',
		isOwner: () => true,
		modulesKnown: () => gate.known,
		isModuleEnabled: () => gate.enabled,
		currentRouteName: () => 'WorkspaceTasksList',
		navigate: jest.fn(),
		isBusy: () => false,
		overlay: () => ({ adapter: { show: jest.fn(), refresh: jest.fn(), hide: jest.fn() } }),
		setTimer: () => () => {},
		hasPersonas,
	};
	return { engine: createTours(deps), bus, gate };
};

const hubIds = (engine: ReturnType<typeof make>['engine']) =>
	buildHubModel(engine.api.hubItems.value, { canManageModules: true, isModuleEnabled: () => true })
		.groups.flatMap((g) => g.rows.map((r) => r.id));

describe('catalog persona requirement', () => {
	it('marks the persona-dependent tours and leaves personas.create free', () => {
		const needing = TOURS.filter((t) => t.requires?.needs?.includes('persona')).map((t) => t.id);
		expect(needing.sort()).toEqual(['personas.assign', 'personas.skills', 'personas.token']);
	});
});

describe('hub persona gating', () => {
	it('hides persona tours until a persona exists, keeps personas.create', async () => {
		const h = make(() => Promise.resolve(false));
		await h.engine.init();
		await h.engine.api.ensurePersonas();
		const ids = hubIds(h.engine);
		expect(ids).toContain('personas.create');
		expect(ids).not.toContain('personas.token');
		expect(ids).not.toContain('personas.skills');
		expect(ids).not.toContain('personas.assign');
	});

	it('shows them after persona.created', async () => {
		const h = make(() => Promise.resolve(false));
		await h.engine.init();
		await h.engine.api.ensurePersonas();
		h.bus.emit('persona.created', {});
		await flush();
		const ids = hubIds(h.engine);
		expect(ids).toEqual(expect.arrayContaining(['personas.token', 'personas.skills', 'personas.assign']));
	});

	it('shows them when the list already has a persona', async () => {
		const h = make(() => Promise.resolve(true));
		await h.engine.init();
		await h.engine.api.ensurePersonas();
		expect(hubIds(h.engine)).toContain('personas.token');
	});

	it('does not refetch once a persona is known', async () => {
		const fetcher = jest.fn().mockResolvedValue(true);
		const h = make(fetcher);
		await h.engine.init();
		await h.engine.api.ensurePersonas();
		await h.engine.api.ensurePersonas();
		expect(fetcher).toHaveBeenCalledTimes(1);
	});

	it('keeps the answer unknown and the rows visible when the request fails', async () => {
		const fetcher = jest.fn().mockRejectedValue(new Error('x'));
		const h = make(fetcher);
		await h.engine.init();
		await h.engine.api.ensurePersonas();
		expect(hubIds(h.engine)).toContain('personas.token');
		await h.engine.api.ensurePersonas();
		expect(fetcher).toHaveBeenCalledTimes(2);
	});

	it('does not ask while the module list is unknown or personas is off', async () => {
		const fetcher = jest.fn().mockResolvedValue(false);
		const h = make(fetcher, { known: false, enabled: true });
		await h.engine.init();
		await h.engine.api.ensurePersonas();
		expect(fetcher).not.toHaveBeenCalled();
		h.gate.known = true;
		h.gate.enabled = false;
		await h.engine.api.ensurePersonas();
		expect(fetcher).not.toHaveBeenCalled();
	});

	it('asks again once modules become known', async () => {
		const fetcher = jest.fn().mockResolvedValue(false);
		const h = make(fetcher, { known: false, enabled: true });
		await h.engine.init();
		await h.engine.api.ensurePersonas();
		h.gate.known = true;
		h.engine.modulesChanged();
		await flush();
		expect(fetcher).toHaveBeenCalledTimes(1);
		expect(hubIds(h.engine)).not.toContain('personas.token');
	});

	it('resets the answer when the workspace modules reload', async () => {
		const fetcher = jest.fn().mockResolvedValueOnce(true).mockResolvedValueOnce(false);
		const h = make(fetcher);
		await h.engine.init();
		await h.engine.api.ensurePersonas();
		expect(hubIds(h.engine)).toContain('personas.token');
		h.gate.known = false;
		h.engine.modulesChanged();
		h.gate.known = true;
		h.engine.modulesChanged();
		await flush();
		expect(fetcher).toHaveBeenCalledTimes(2);
		expect(hubIds(h.engine)).not.toContain('personas.token');
	});

	it('looks the answer up lazily when starting a persona tour', async () => {
		const fetcher = jest.fn().mockResolvedValue(false);
		const h = make(fetcher);
		await h.engine.init();
		await h.engine.api.start('personas.token', 'hub');
		expect(fetcher).toHaveBeenCalledTimes(1);
		expect(h.engine.api.active.value).toBeNull();
	});

	it('refuses to start a persona tour without one', async () => {
		const h = make(() => Promise.resolve(false));
		await h.engine.init();
		await h.engine.api.ensurePersonas();
		await h.engine.api.start('personas.token', 'hub');
		expect(h.engine.api.active.value).toBeNull();
	});
});
