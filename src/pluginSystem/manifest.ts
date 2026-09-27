export const PLUGIN_API_VERSION = '1.1';

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
] as const;

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
	contributes: {
		boardCardBadges: { id: string }[];
		statusBarItems: { id: string }[];
		commands: { id: string; title: string }[];
		/** With `ui`, the view is the plugin's own HTML page, opened in a separate window. */
		views: { id: string; title: string; ui?: string }[];
		taskPanelSections: { id: string; title: string }[];
		settings: {
			type: 'object';
			properties: Record<string, SettingSchema>;
		} | null;
	};
}

const PLUGIN_ID = /^[a-z0-9][a-z0-9-]*\.[a-z0-9][a-z0-9-]*$/;
const LOCAL_ID = /^[a-z0-9][a-z0-9-]*$/;
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

/** Same major, and no newer minor than this app implements: a ^1.2 plugin would call methods 1.1 lacks. */
const engineSupported = (range: unknown) => {
	const match =
		typeof range === 'string' ? range.match(/^\^?(\d+)\.(\d+)/) : null;
	const [major, minor] = PLUGIN_API_VERSION.split('.').map(Number);
	return !!match && Number(match[1]) === major && Number(match[2]) <= minor;
};

const DOMAIN = /^(?=.{1,253}$)([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,63}$/;

const parseDomain = (value: unknown): string =>
	typeof value === 'string' && DOMAIN.test(value)
		? value
		: fail(`links domain ${String(value)} must be a host name like example.com`);

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
	const allowedDomains = [
		...new Set(
			list(raw.links?.allowedDomains, 'links.allowedDomains', parseDomain),
		),
	];
	if (allowedDomains.length && !permissions.includes('links:open'))
		fail('links.allowedDomains needs the links:open permission');
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
		contributes: {
			boardCardBadges: list(c.boardCardBadges, 'boardCardBadges', (item) => ({
				id: localId(item, 'boardCardBadges'),
			})),
			statusBarItems: list(c.statusBarItems, 'statusBarItems', (item) => ({
				id: localId(item, 'statusBarItems'),
			})),
			commands: list(c.commands, 'commands', (item) => {
				const commandId = text(item?.id, 'commands.id', 120);
				if (!commandId.startsWith(`${id}.`))
					fail(`command ${commandId} must start with ${id}.`);
				return {
					id: commandId,
					title: text(item?.title, 'commands.title', 80),
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
			settings: parseSettings(c.settings),
		},
	};
};
