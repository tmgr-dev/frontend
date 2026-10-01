export const PLUGIN_API_VERSION = '1.5';

export const PERMISSIONS = [
	'tasks:read',
	'tasks:write',
	'statuses:read',
	'categories:read',
	'time:read',
	'time:write',
	'comments:read',
	'comments:write',
	'notifications',
	'files:export',
	'files:attachments',
	'files:pick',
	'statuses:write',
	'categories:write',
	'relations:read',
	'relations:write',
	'agent_work:read',
	'agent_work:write',
	'alarms',
	'tray',
	'deeplinks',
	'links:open',
	'routines:read',
	'routines:write',
	'views:badge',
	'pages:read',
	'pages:write',
	'pages:sections',
] as const;

const PAGE_PERMISSIONS = ['pages:read', 'pages:write', 'pages:sections'] as const;

export type Permission = (typeof PERMISSIONS)[number];

export interface SettingSchema {
	type: 'number' | 'string' | 'boolean';
	title?: string;
	description?: string;
	default?: number | string | boolean;
}

export interface PluginManifest {
	id: string;
	name: string;
	version: string;
	publisher: string;
	description: string;
	main: string;
	permissions: Permission[];
	/** Only origins on this computer; a plugin never reaches the internet. */
	network: { allowedOrigins: string[] };
	/** Hosts a `link` node may open in the system browser (https only); needs links:open. */
	links: { allowedDomains: string[] };
	/** Declares an external program the user installs themselves; enables tmgr.localAccess.requestConnection. */
	companion: { description: string; homepage?: string } | null;
	contributes: {
		boardCardBadges: { id: string }[];
		statusBarItems: { id: string }[];
		/** At most 5, each set with `tmgr.ui.setTrayItem`; needs the tray permission. */
		trayItems: { id: string }[];
		/** `deepLink` lets `tmgr://plugin/<id>/command/<local id>` run it; needs the deeplinks permission. */
		commands: { id: string; title: string; deepLink?: boolean }[];
		/** With `ui`, the view is the plugin's own HTML page, opened in a separate window. */
		views: { id: string; title: string; ui?: string }[];
		taskPanelSections: { id: string; title: string }[];
		/** Extra board quick-filters, each keyed to a badge (and optionally its `key`) from this plugin's own boardCardBadges. */
		boardFilters: { id: string; title: string; badge: string; key?: string }[];
		settings: {
			type: 'object';
			properties: Record<string, SettingSchema>;
		} | null;
	};
	/** The minor from engines.tmgr (e.g. '^1.2' → 2); gates which UiNode fields sanitizeTree emits. */
	apiMinor: number;
}

export const PLUGIN_ID = /^[a-z0-9][a-z0-9-]*\.[a-z0-9][a-z0-9-]*$/;
export const LOCAL_ID = /^[a-z0-9][a-z0-9-]*$/;
const LOOPBACK_ORIGIN =
	/^http:\/\/(localhost|127\.0\.0\.1|\[::1\]):([1-9]\d{0,4})\/?$/;

const parseOrigin = (value: unknown): string => {
	const match = typeof value === 'string' ? value.match(LOOPBACK_ORIGIN) : null;
	if (!match || Number(match[2]) < 1 || Number(match[2]) > 65535) {
		fail(
			`network origin ${String(
				value,
			)} must be http://localhost:<port> (or 127.0.0.1 / [::1])`,
		);
	}
	// Written as the browser's URL.origin writes it, so the broker's comparison can match: port 80 is implied.
	return `http://${match![1]}${match![2] === '80' ? '' : `:${match![2]}`}`;
};

const UI_PAGE = /^ui\/[a-z0-9][a-z0-9_-]*\.html$/i;

const SEMVER = /^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/;

const fail = (message: string): never => {
	throw new Error(`Invalid plugin manifest: ${message}`);
};

const text = (value: unknown, field: string, max = 200): string =>
	typeof value === 'string' && value.trim() && value.length <= max
		? value.trim()
		: fail(`${field} must be a non-empty string`);

const list = <T>(
	value: unknown,
	field: string,
	parse: (item: any) => T,
): T[] => {
	if (value === undefined) return [];
	if (!Array.isArray(value) || value.length > 50)
		fail(`${field} must be a list`);
	return (value as unknown[]).map(parse);
};

const ENGINE_RANGE = /^\^?(\d+)\.(\d+)/;

/** Same major, and no newer minor than this app implements: a ^1.3 plugin would call methods 1.2 lacks. */
const engineSupported = (range: unknown) => {
	const match = typeof range === 'string' ? range.match(ENGINE_RANGE) : null;
	const [major, minor] = PLUGIN_API_VERSION.split('.').map(Number);
	return !!match && Number(match[1]) === major && Number(match[2]) <= minor;
};

const engineMinor = (range: unknown): number =>
	Number((range as string).match(ENGINE_RANGE)![2]);

const DOMAIN = /^(?=.{1,253}$)([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,63}$/;

const parseDomain = (value: unknown): string =>
	typeof value === 'string' && DOMAIN.test(value)
		? value
		: fail(`links domain ${String(value)} must be a host name like example.com`);

const HTTPS_URL = /^https:\/\/.+/;

const parseCompanion = (value: unknown): PluginManifest['companion'] => {
	if (value === undefined || value === null) return null;
	if (typeof value !== 'object') fail('companion must be an object');
	const v = value as Record<string, unknown>;
	const description = text(v.description, 'companion.description', 200);
	if (v.homepage === undefined) return { description };
	if (typeof v.homepage !== 'string' || !HTTPS_URL.test(v.homepage))
		fail('companion.homepage must be an https URL');
	return { description, homepage: v.homepage as string };
};

const parseSettings = (
	value: any,
): PluginManifest['contributes']['settings'] => {
	if (value === undefined || value === null) return null;
	if (value?.type !== 'object' || typeof value.properties !== 'object') {
		fail('contributes.settings must be an object schema');
	}
	const properties: Record<string, SettingSchema> = {};
	for (const [key, property] of Object.entries<any>(value.properties)) {
		if (!LOCAL_ID.test(key.toLowerCase())) fail(`setting ${key}`);
		if (!['number', 'string', 'boolean'].includes(property?.type))
			fail(`setting ${key} type`);
		properties[key] = {
			type: property.type,
			...(typeof property.title === 'string' ? { title: property.title } : {}),
			...(typeof property.description === 'string'
				? { description: property.description }
				: {}),
			...(typeof property.default === property.type
				? { default: property.default }
				: {}),
		};
	}
	return { type: 'object', properties };
};

/** Validates a manifest from any source (built-in or a folder) before anything of the plugin runs. */
export const parseManifest = (raw: any): PluginManifest => {
	if (!raw || typeof raw !== 'object') fail('not an object');
	const id = text(raw.id, 'id', 80);
	if (!PLUGIN_ID.test(id)) fail(`id "${id}" must look like publisher.name`);
	const version = text(raw.version, 'version', 40);
	if (!SEMVER.test(version)) fail(`version "${version}" is not semver`);
	if (!engineSupported(raw.engines?.tmgr))
		fail(`engines.tmgr must be ^1.0 up to ^${PLUGIN_API_VERSION}`);
	const permissions = list(raw.permissions, 'permissions', (permission) =>
		(PERMISSIONS as readonly string[]).includes(permission)
			? (permission as Permission)
			: fail(`unknown permission ${permission}`),
	);
	const localId = (item: any, field: string) => {
		const value = text(item?.id, `${field}.id`, 60);
		return LOCAL_ID.test(value) ? value : fail(`${field} id "${value}"`);
	};
	const c = raw.contributes ?? {};
	const boardCardBadges = list(c.boardCardBadges, 'boardCardBadges', (item) => ({
		id: localId(item, 'boardCardBadges'),
	}));
	const boardCardBadgeIds = new Set(boardCardBadges.map((b) => b.id));
	const allowedDomains = [
		...new Set(
			list(raw.links?.allowedDomains, 'links.allowedDomains', parseDomain),
		),
	];
	if (allowedDomains.length && !permissions.includes('links:open'))
		fail('links.allowedDomains needs the links:open permission');
	const apiMinor = engineMinor(raw.engines.tmgr);
	if (permissions.includes('views:badge') && apiMinor < 4)
		fail('views:badge needs engines.tmgr ^1.4');
	for (const permission of PAGE_PERMISSIONS)
		if (permissions.includes(permission) && apiMinor < 5)
			fail(`${permission} needs engines.tmgr ^1.5`);
	return {
		id,
		name: text(raw.name, 'name', 80),
		version,
		publisher: text(raw.publisher ?? id.split('.')[0], 'publisher', 80),
		description:
			typeof raw.description === 'string' ? raw.description.slice(0, 500) : '',
		main: text(raw.main ?? 'main.js', 'main', 80),
		permissions: [...new Set(permissions)],
		network: {
			allowedOrigins: [
				...new Set(
					list(
						raw.network?.allowedOrigins,
						'network.allowedOrigins',
						parseOrigin,
					),
				),
			],
		},
		links: { allowedDomains },
		companion: parseCompanion(raw.companion),
		apiMinor,
		contributes: {
			boardCardBadges,
			statusBarItems: list(c.statusBarItems, 'statusBarItems', (item) => ({
				id: localId(item, 'statusBarItems'),
			})),
			trayItems: (() => {
				const items = list(c.trayItems, 'trayItems', (item) => ({
					id: localId(item, 'trayItems'),
				}));
				if (items.length > 5) fail('contributes.trayItems must have at most 5 entries');
				return items;
			})(),
			commands: list(c.commands, 'commands', (item) => {
				const commandId = text(item?.id, 'commands.id', 120);
				if (!commandId.startsWith(`${id}.`))
					fail(`command ${commandId} must start with ${id}.`);
				if (item?.deepLink !== undefined && typeof item.deepLink !== 'boolean')
					fail(`commands.deepLink must be a boolean`);
				const deepLink = item?.deepLink === true;
				if (deepLink && !LOCAL_ID.test(commandId.slice(id.length + 1)))
					fail(`command ${commandId} needs a plain local id to be linkable`);
				return {
					id: commandId,
					title: text(item?.title, 'commands.title', 80),
					...(deepLink ? { deepLink: true as const } : {}),
				};
			}),
			views: list(c.views, 'views', (item) => {
				const view = {
					id: localId(item, 'views'),
					title: text(item?.title, 'views.title', 60),
				};
				if (item?.ui === undefined) return view;
				if (typeof item.ui !== 'string' || !UI_PAGE.test(item.ui)) {
					fail(
						`views.ui "${String(item.ui)}" must be a file like ui/page.html`,
					);
				}
				return { ...view, ui: item.ui as string };
			}),
			taskPanelSections: list(
				c.taskPanelSections,
				'taskPanelSections',
				(item) => ({
					id: localId(item, 'taskPanelSections'),
					title: text(item?.title, 'taskPanelSections.title', 60),
				}),
			),
			boardFilters: list(c.boardFilters, 'boardFilters', (item) => {
				const badge = text(item?.badge, 'boardFilters.badge', 60);
				if (!boardCardBadgeIds.has(badge))
					fail(`boardFilters.badge "${badge}" is not a declared boardCardBadges id`);
				const key = item?.key === undefined ? undefined : text(item.key, 'boardFilters.key', 40);
				if (key !== undefined && !/^[a-z0-9_-]+$/.test(key)) fail('boardFilters.key');
				return {
					id: localId(item, 'boardFilters'),
					title: text(item?.title, 'boardFilters.title', 40),
					badge,
					key,
				};
			}),
			settings: parseSettings(c.settings),
		},
	};
};
