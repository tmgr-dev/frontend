import type {
	CreatePagePayload,
	Page,
	UpdatePagePayload,
} from '@/actions/tmgr/pages';
import { parseTmgrUrl } from '@/utils/pages/tmgrLinks';
import { buildLinkIndex, resolveLink, type LinkIndex } from './importPlan';
import {
	isTmgrTarget,
	rewriteMarkdownLinks,
	splitTargetFragment,
} from './links';
import { baseName, extName } from './paths';
import type {
	CreatedPage,
	ImportOptions,
	ImportPlan,
	ImportResult,
	IoWarning,
	PlannedPage,
} from './types';

export interface ImportUpload {
	name: string;
	mime: string;
	bytes: Uint8Array;
}

export interface ImportApi {
	createPage(payload: CreatePagePayload): Promise<Page>;
	getPage(id: number): Promise<Page>;
	updatePage(id: number, payload: UpdatePagePayload): Promise<Page>;
	uploadFile(pageId: number, file: ImportUpload): Promise<{ id: number }>;
	listChildTitles(parentId: number | null): Promise<string[]>;
	finish?(): void;
}

type Props = Record<string, any>;
type PagePatch = Omit<UpdatePagePayload, 'version'>;

const MIME: Record<string, string> = {
	'.png': 'image/png',
	'.jpg': 'image/jpeg',
	'.jpeg': 'image/jpeg',
	'.gif': 'image/gif',
	'.webp': 'image/webp',
	'.svg': 'image/svg+xml',
	'.bmp': 'image/bmp',
	'.avif': 'image/avif',
	'.pdf': 'application/pdf',
	'.txt': 'text/plain',
	'.csv': 'text/csv',
	'.json': 'application/json',
	'.zip': 'application/zip',
};

const normalize = (title: string): string => title.trim().toLowerCase();

const messageOf = (error: unknown): string => {
	const anyError = error as any;
	const data = anyError?.response?.data;
	return (
		data?.message || data?.error || anyError?.message || 'The request failed'
	);
};

const statusOf = (error: unknown): number | undefined =>
	(error as any)?.response?.status ?? (error as any)?.status;

const isConflict = (error: unknown): error is { current: Page } =>
	(error as any)?.name === 'PageConflictError' &&
	typeof (error as any)?.current?.version === 'number';

const stripIdBound = (type: string, properties: Props): Props => {
	const next = { ...properties };
	if (type === 'person') delete next.user_id;
	if (type === 'meeting') {
		if ('participants' in next) next.participants = [];
		if ('related_tasks' in next) next.related_tasks = [];
	}
	return next;
};

const needsSecondPass = (page: PlannedPage): boolean =>
	/\]\(|\[\[|\]:|tmgr:\/\//.test(page.body) || page.properties !== null;

const applyPolicy = async (
	plan: ImportPlan,
	options: ImportOptions,
	api: ImportApi,
): Promise<{ pages: PlannedPage[]; skipped: string[] }> => {
	const existing = new Set(
		(await api.listChildTitles(options.parentId)).map(normalize),
	);
	const dropped = new Set<string>();
	const skipped: string[] = [];
	const titles = new Map<string, string>();
	for (const page of plan.pages) {
		if (page.parentKey && dropped.has(page.parentKey)) {
			dropped.add(page.key);
			skipped.push(page.title);
			continue;
		}
		if (page.parentKey !== null || !existing.has(normalize(page.title))) {
			continue;
		}
		if (options.policy === 'skip') {
			dropped.add(page.key);
			skipped.push(page.title);
		} else if (options.policy === 'rename') {
			let index = 2;
			let title = `${page.title} (${index})`;
			while (existing.has(normalize(title))) {
				index += 1;
				title = `${page.title} (${index})`;
			}
			existing.add(normalize(title));
			titles.set(page.key, title);
		}
	}
	return {
		pages: plan.pages
			.filter((page) => !dropped.has(page.key))
			.map((page) =>
				titles.has(page.key)
					? { ...page, title: titles.get(page.key) as string }
					: page,
			),
		skipped,
	};
};

interface Run {
	api: ImportApi;
	plan: ImportPlan;
	warnings: IoWarning[];
	idByKey: Map<string, number>;
	idBySource: Map<string, number>;
	idsBySource: Map<number, number[]>;
}

const mappedPageId = (
	run: Run,
	page: PlannedPage,
	oldId: number,
): number | undefined => {
	const own = run.idBySource.get(`${page.sourceWorkspace ?? ''}:${oldId}`);
	if (own !== undefined) return own;
	if (page.sourceWorkspace === null) {
		const candidates = run.idsBySource.get(oldId) ?? [];
		if (candidates.length === 1) return candidates[0];
	}
	return undefined;
};

const finalProperties = (
	run: Run,
	page: PlannedPage,
	sameWorkspace: boolean,
	dropped: { count: number },
): Props | null => {
	if (!page.properties) return null;
	const props = { ...page.properties };
	if (page.type === 'person' && !sameWorkspace && props.user_id != null) {
		delete props.user_id;
		dropped.count += 1;
	}
	if (page.type === 'meeting') {
		if (Array.isArray(props.participants)) {
			props.participants = props.participants.flatMap((item: unknown) => {
				const parsed = typeof item === 'string' ? parseTmgrUrl(item) : null;
				if (parsed?.form === 'storage' && parsed.kind === 'page') {
					const mapped = mappedPageId(run, page, Number(parsed.id));
					if (mapped !== undefined) return [`tmgr://page/${mapped}`];
				}
				if (sameWorkspace) return [item];
				dropped.count += 1;
				return [];
			});
		}
		if (!sameWorkspace && props.related_tasks?.length) {
			props.related_tasks = [];
			dropped.count += 1;
		}
	}
	return props;
};

const plainLink = (label: string, url: string): string =>
	`[${label.replace(/([[\]])/g, '\\$1')}](${url})`;

const rewriteBody = async (
	run: Run,
	page: PlannedPage,
	pageId: number,
	index: LinkIndex,
	sameWorkspace: boolean,
): Promise<string> => {
	const uploaded = new Map<string, number | null>();
	const neutralized = { count: 0 };

	const wanted = new Set<string>();
	rewriteMarkdownLinks(page.body, (token) => {
		const resolved = resolveLink(index, page.path, token);
		if (resolved.type === 'file') wanted.add(resolved.path);
		return null;
	});
	for (const path of wanted) {
		try {
			const { id } = await run.api.uploadFile(pageId, {
				name: baseName(path),
				mime: MIME[extName(path)] ?? 'application/octet-stream',
				bytes: run.plan.files[path].bytes,
			});
			uploaded.set(path, id);
		} catch (error) {
			uploaded.set(path, null);
			run.warnings.push({
				path,
				message: `Could not upload the file: ${messageOf(error)}`,
			});
		}
	}

	const rewritten = rewriteMarkdownLinks(page.body, (token) => {
		const target = token.target.trim();
		if (isTmgrTarget(target)) {
			const parsed = parseTmgrUrl(splitTargetFragment(target).base);
			if (parsed?.form === 'storage' && parsed.kind === 'page') {
				const mapped = mappedPageId(run, page, Number(parsed.id));
				if (mapped !== undefined) return token.build(`tmgr://page/${mapped}`);
			}
			if (sameWorkspace) return null;
			neutralized.count += 1;
			return token.kind === 'definition' ? '' : token.label;
		}
		const resolved = resolveLink(index, page.path, token);
		const wiki = token.kind === 'wikilink' || token.kind === 'embed';
		if (resolved.type === 'page') {
			const id = run.idByKey.get(resolved.key);
			if (id === undefined) return null;
			const url = `tmgr://page/${id}`;
			if (!wiki) return token.build(url);
			return plainLink(token.label || token.target.split('#')[0], url);
		}
		if (resolved.type === 'file') {
			const id = uploaded.get(resolved.path);
			if (id === null || id === undefined) return null;
			const url = `tmgr://file/${id}`;
			if (!wiki) return token.build(url);
			const name = baseName(resolved.path);
			if (token.kind === 'embed' && resolved.image) {
				const alt = /^\d+(x\d+)?$/.test(token.label) ? '' : token.label;
				return `!${plainLink(alt, url)}`;
			}
			return plainLink(
				token.kind === 'embed' ? name : token.label || name,
				url,
			);
		}
		return null;
	});

	if (neutralized.count) {
		run.warnings.push({
			path: page.path,
			message: `${neutralized.count} tmgr:// reference${
				neutralized.count === 1 ? '' : 's'
			} to other workspaces or unknown objects replaced with plain text`,
		});
	}
	return rewritten;
};

const updateWithFreshVersion = async (
	api: ImportApi,
	id: number,
	patch: PagePatch,
): Promise<void> => {
	const fresh = await api.getPage(id);
	try {
		await api.updatePage(id, { ...patch, version: fresh.version });
	} catch (error) {
		if (!isConflict(error)) throw error;
		await api.updatePage(id, { ...patch, version: error.current.version });
	}
};

export const runImportWith = async (
	api: ImportApi,
	plan: ImportPlan,
	options: ImportOptions,
): Promise<ImportResult> => {
	const created: CreatedPage[] = [];
	const warnings: IoWarning[] = [];
	let error: string | null = null;
	let skipped: string[] = [];
	try {
		const applied = await applyPolicy(plan, options, api);
		skipped = applied.skipped;
		const pages = applied.pages;
		const run: Run = {
			api,
			plan,
			warnings,
			idByKey: new Map(),
			idBySource: new Map(),
			idsBySource: new Map(),
		};
		const total = pages.length + pages.filter(needsSecondPass).length;
		let done = 0;
		const progress = (current: string) =>
			options.onProgress?.({ done, total, current });
		const firstBody = new Map<string, string>();
		const firstProps = new Map<string, Props | null>();

		progress('');
		for (const page of pages) {
			const parentId =
				page.parentKey === null
					? options.parentId
					: run.idByKey.get(page.parentKey) ?? options.parentId;
			const body = page.body.includes('tmgr://') ? '' : page.body;
			const props = page.properties
				? stripIdBound(page.type, page.properties)
				: null;
			const payload: CreatePagePayload = {
				title: page.title,
				type: page.type,
				parent_id: parentId,
				body,
			};
			if (props && Object.keys(props).length) payload.properties = props;
			let result: Page;
			try {
				try {
					result = await api.createPage(payload);
				} catch (failure) {
					if (!payload.properties || statusOf(failure) !== 422) throw failure;
					warnings.push({
						path: page.path,
						message: `Properties were rejected and "${page.title}" was created without them`,
					});
					delete payload.properties;
					result = await api.createPage(payload);
				}
			} catch (failure) {
				error = `Could not create "${page.title}": ${messageOf(failure)}`;
				break;
			}
			created.push({
				key: page.key,
				id: result.id,
				slug: result.slug,
				title: result.title,
			});
			run.idByKey.set(page.key, result.id);
			firstBody.set(page.key, body);
			firstProps.set(page.key, payload.properties ?? null);
			if (page.sourceId !== null) {
				run.idBySource.set(
					`${page.sourceWorkspace ?? ''}:${page.sourceId}`,
					result.id,
				);
				run.idsBySource.set(page.sourceId, [
					...(run.idsBySource.get(page.sourceId) ?? []),
					result.id,
				]);
			}
			done += 1;
			progress(page.title);
		}

		if (!error) {
			const index = buildLinkIndex(pages, plan.files);
			for (const page of pages.filter(needsSecondPass)) {
				const id = run.idByKey.get(page.key) as number;
				progress(page.title);
				try {
					const sameWorkspace = page.sourceWorkspace === options.workspaceCode;
					const body = await rewriteBody(run, page, id, index, sameWorkspace);
					const dropped = { count: 0 };
					const props = finalProperties(run, page, sameWorkspace, dropped);
					if (dropped.count) {
						warnings.push({
							path: page.path,
							message: `Dropped ${dropped.count} reference${
								dropped.count === 1 ? '' : 's'
							} to members or tasks of another workspace`,
						});
					}
					const patch: PagePatch = {};
					if (body !== firstBody.get(page.key)) patch.body = body;
					if (
						props &&
						Object.keys(props).length &&
						JSON.stringify(props) !== JSON.stringify(firstProps.get(page.key))
					) {
						patch.properties = props;
					}
					if (patch.body !== undefined || patch.properties) {
						try {
							await updateWithFreshVersion(api, id, patch);
						} catch (failure) {
							if (!patch.properties || statusOf(failure) !== 422) throw failure;
							warnings.push({
								path: page.path,
								message: 'Properties were rejected and left unset',
							});
							delete patch.properties;
							if (patch.body !== undefined) {
								await updateWithFreshVersion(api, id, patch);
							}
						}
					}
				} catch (failure) {
					warnings.push({
						path: page.path,
						message: `Could not finish links and files: ${messageOf(failure)}`,
					});
				}
				done += 1;
				progress(page.title);
			}
		}
	} catch (failure) {
		error = messageOf(failure);
	} finally {
		api.finish?.();
	}
	return { created, skipped, warnings, error };
};
