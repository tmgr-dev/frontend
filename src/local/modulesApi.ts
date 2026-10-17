import { readUserToggles } from './accountless';
import {
	LOCAL_DEFAULTS,
	MODULES,
	PACKS,
	PRESETS,
	type CatalogModule,
} from './modulesCatalog';
import type { LocalRouter } from './router';
import type { LocalContext } from './types';

const FEATURE_PREFIX = 'feature.';
const HIDDEN_KEY = 'modules.hidden';

const readMeta = async (ctx: LocalContext, key: string) => {
	const [row] = await ctx.db.select<{ value: string }>(
		`SELECT value FROM meta WHERE key = ?`,
		[key],
	);
	return row?.value;
};

const writeMeta = (ctx: LocalContext, key: string, value: string) =>
	ctx.db.execute(
		`INSERT INTO meta (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value`,
		[key, value],
	);

const toggleable = (module: CatalogModule) =>
	module.scope === 'workspace' && module.local && !module.core;

const hiddenKeys = async (
	ctx: LocalContext,
): Promise<Record<string, boolean>> => {
	try {
		const parsed = JSON.parse((await readMeta(ctx, HIDDEN_KEY)) ?? '{}');
		return parsed && typeof parsed === 'object' ? parsed : {};
	} catch {
		return {};
	}
};

export const localModulesPayload = async (ctx: LocalContext) => {
	const rows = await ctx.db.select<{ key: string; value: string }>(
		`SELECT key, value FROM meta WHERE key LIKE '${FEATURE_PREFIX}%'`,
	);
	const stored = new Map(
		rows.map((row) => [row.key.slice(FEATURE_PREFIX.length), row.value]),
	);
	const hidden = await hiddenKeys(ctx);
	const userToggles = readUserToggles();
	const modules = MODULES.filter(
		(module) => module.scope === 'workspace' || ctx.accountless,
	).map((module) => {
		let enabled: boolean;
		if (!module.local) enabled = false;
		else if (module.core) enabled = true;
		else if (module.scope === 'user') enabled = userToggles.mcp;
		else
			enabled = stored.has(module.key)
				? stored.get(module.key) === '1'
				: LOCAL_DEFAULTS[module.key] ?? module.default;
		return {
			...module,
			enabled,
			hidden: module.scope === 'workspace' && hidden[module.key] === true,
			...(module.local ? {} : { requires_account: true }),
		};
	});
	const workflow = modules.find((module) => module.key === 'workflow');
	const board = modules.find((module) => module.key === 'board');
	if (board && workflow) board.enabled = workflow.enabled;
	return {
		configured: true,
		can_manage: true,
		enforcement: 'report',
		packs: PACKS,
		presets: PRESETS,
		modules,
	};
};

const applyChoice = async (ctx: LocalContext, body: any) => {
	const writes = new Map<string, boolean>();
	const preset = PRESETS.find((p) => p.key === body?.preset);
	if (preset) {
		for (const module of MODULES.filter(toggleable))
			writes.set(module.key, preset.modules.includes(module.key));
	}
	if (body?.modules && typeof body.modules === 'object') {
		for (const [key, value] of Object.entries(body.modules)) {
			if (typeof value === 'boolean') writes.set(key, value);
		}
		if (body.modules.board === true) writes.set('workflow', true);
	}
	await writeFeatures(ctx, Object.fromEntries(writes));
};

export const writeFeatures = async (
	ctx: LocalContext,
	features: Record<string, unknown>,
) => {
	for (const module of MODULES.filter(toggleable)) {
		const value = features[module.key];
		if (typeof value === 'boolean')
			await writeMeta(ctx, `${FEATURE_PREFIX}${module.key}`, value ? '1' : '0');
	}
};

export const addModulesRoutes = (router: LocalRouter) => {
	router
		.add('GET', 'workspaces/:wid/modules', ({ ctx }) =>
			localModulesPayload(ctx),
		)
		.add('PUT', 'workspaces/:wid/modules', async ({ ctx, body }) => {
			await applyChoice(ctx, body);
			return localModulesPayload(ctx);
		})
		.add('PUT', 'workspaces/:wid/modules/hidden', async ({ ctx, body }) => {
			const hidden = await hiddenKeys(ctx);
			for (const [key, value] of Object.entries(body ?? {})) {
				if (
					typeof value !== 'boolean' ||
					!MODULES.some((m) => m.key === key && m.scope === 'workspace')
				)
					continue;
				if (value) hidden[key] = true;
				else delete hidden[key];
			}
			await writeMeta(ctx, HIDDEN_KEY, JSON.stringify(hidden));
			return localModulesPayload(ctx);
		});
};
