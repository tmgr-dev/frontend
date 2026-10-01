'use strict';
// In-memory pages for the test host: enough of the server's rules (versions, 409 page_conflict, managed
// sections and their owners, append by heading) for a plugin's own logic to be tested.
const SECTION =
	/(<!--\s*tmgr:section\s+id="([^"]+)"\s+owner="([^"]*)"\s*-->)([\s\S]*?)(<!--\s*\/tmgr:section\s*-->)/g;

const sectionsOf = (body) =>
	[...body.matchAll(SECTION)].map((m) => {
		const heading = m[4].match(/^\s*#{1,6}\s+(.+?)\s*$/m);
		return { id: m[2], owner: m[3], heading: heading ? heading[1] : null };
	});

const slugOf = (title, taken) => {
	const base =
		title
			.toLowerCase()
			.replace(/[^a-z0-9]+/g, '-')
			.replace(/^-|-$/g, '') || 'page';
	let slug = base;
	for (let n = 2; taken.has(slug); n++) slug = `${base}-${n}`;
	return slug;
};

const summary = (page) => ({
	id: page.id,
	title: page.title,
	slug: page.slug,
	type: page.type,
	parent_id: page.parent_id,
	position: page.position,
	pinned: false,
	updated_at: page.updated_at,
});

const appendMarkdown = (body, fields) => {
	const text = fields.markdown.trim();
	if (!fields.heading) return `${body.replace(/\s+$/, '')}\n\n${text}\n`;
	const lines = body.split('\n');
	const wanted = fields.heading.trim().toLowerCase();
	const at = lines.findIndex(
		(l) =>
			/^##\s/.test(l) &&
			l
				.replace(/^##\s+/, '')
				.trim()
				.toLowerCase() === wanted,
	);
	if (at === -1) {
		if (!fields.create_heading) {
			const error = new Error(`heading ${fields.heading} not found`);
			error.code = 'INVALID_PARAMS';
			throw error;
		}
		return `${body.replace(/\s+$/, '')}\n\n## ${fields.heading}\n\n${text}\n`;
	}
	let end = lines.findIndex((l, i) => i > at && /^#{1,2}\s/.test(l));
	if (end === -1) end = lines.length;
	const before = lines.slice(0, end).join('\n').replace(/\s+$/, '');
	return (
		`${before}\n${text}\n\n${lines.slice(end).join('\n')}`.replace(/\s+$/, '') +
		'\n'
	);
};

const createMockPages = (state, clock, PluginError, pluginId) => {
	const find = (ref) => {
		const page = state.pages.find(
			(p) =>
				!p.deleted_at &&
				(typeof ref === 'number' ? p.id === ref : p.slug === ref),
		);
		if (!page) throw new PluginError('HOST_ERROR', 'page not found');
		return page;
	};
	const view = (page) => ({
		...summary(page),
		workspace_id: -1,
		body: page.body,
		properties: { ...page.properties },
		version: page.version,
		author: {
			kind: page.author_kind,
			id: page.author_id,
			name: page.author_id,
		},
		updated_by: { kind: 'plugin', id: pluginId, name: pluginId },
		created_at: page.created_at,
		backlinks: [],
		sections: sectionsOf(page.body),
	});
	const touch = (page, patch) => {
		Object.assign(page, patch, {
			version: page.version + 1,
			updated_at: new Date(clock()).toISOString(),
		});
		return view(page);
	};
	const checkVersion = (page, version) => {
		if (page.version !== version)
			throw new PluginError('page_conflict', 'page was changed', view(page));
	};
	const seed = (fields) => {
		const created = new Date(clock()).toISOString();
		const page = {
			id: state.nextPageId++,
			title: fields.title,
			slug:
				fields.slug ??
				slugOf(fields.title, new Set(state.pages.map((p) => p.slug))),
			type: fields.type ?? 'plain',
			parent_id: fields.parent_id ?? null,
			position: state.pages.length,
			body: fields.body ?? '',
			properties: fields.properties ?? {},
			version: 1,
			author_kind: fields.author_kind ?? 'user',
			author_id: fields.author_id ?? '1',
			created_at: created,
			updated_at: created,
			deleted_at: null,
		};
		state.pages.push(page);
		return page;
	};
	return {
		seed,
		api: {
			async pagesSearch(q, opts) {
				const needle = q.toLowerCase();
				return state.pages
					.filter(
						(p) =>
							!p.deleted_at &&
							(!opts.type || p.type === opts.type) &&
							(p.title.toLowerCase().includes(needle) ||
								p.body.toLowerCase().includes(needle)),
					)
					.slice(0, opts.limit ?? 20)
					.map((p) => ({
						id: p.id,
						slug: p.slug,
						title: p.title,
						type: p.type,
						snippet: p.body.slice(0, 120),
						updated_at: p.updated_at,
					}));
			},
			async pagesTree() {
				return state.pages.filter((p) => !p.deleted_at).map(summary);
			},
			async pagesGet(ref) {
				return view(find(ref));
			},
			async pagesCreate(fields) {
				const page = seed({
					...fields,
					author_kind: 'plugin',
					author_id: pluginId,
				});
				return view(page);
			},
			async pagesUpdate(id, fields) {
				const page = find(id);
				checkVersion(page, fields.version);
				const { version, summary: _summary, ...patch } = fields;
				return touch(page, patch);
			},
			async pagesAppend(id, fields) {
				const page = find(id);
				return touch(page, { body: appendMarkdown(page.body, fields) });
			},
			async pagesSetSection(id, sectionId, markdown, _summary, heading) {
				const page = find(id);
				let found = false;
				const body = page.body.replace(
					SECTION,
					(whole, open, sid, owner, _content, close) => {
						if (sid !== sectionId) return whole;
						found = true;
						if (owner !== `plugin:${pluginId}`)
							throw new PluginError('PERMISSION_DENIED', 'section_forbidden');
						return `${open}\n${markdown.trim()}\n${close}`;
					},
				);
				if (!found) {
					if (page.type === 'context')
						throw new PluginError('PERMISSION_DENIED', 'section_forbidden');
					if (!/^[a-z0-9][a-z0-9-]{0,63}$/.test(sectionId))
						throw new PluginError('INVALID_PARAMS', 'invalid_section_id');
					const title = String(heading || sectionId).trim();
					if (
						title.length > 200 ||
						/[\r\n]/.test(title) ||
						title.includes('<!--')
					)
						throw new PluginError('INVALID_PARAMS', 'invalid_heading');
					const head = page.body.replace(/\s+$/, '');
					const block = `<!-- tmgr:section id="${sectionId}" owner="plugin:${pluginId}" -->\n## ${title}\n\n${markdown.trim()}\n<!-- /tmgr:section -->\n`;
					return touch(page, { body: head ? `${head}\n\n${block}` : block });
				}
				return touch(page, { body });
			},
			async pageDataGet(pageId, key) {
				find(pageId);
				return state.pageData[`${pageId}:${key}`] ?? null;
			},
			async pageDataSet(pageId, key, json) {
				find(pageId);
				state.pageData[`${pageId}:${key}`] = json;
				return { ok: true };
			},
			async pageDataDelete(pageId, key) {
				find(pageId);
				delete state.pageData[`${pageId}:${key}`];
				return { ok: true };
			},
			async pageDataGetMany(pageIds, key) {
				const result = {};
				for (const id of pageIds)
					result[id] = state.pageData[`${id}:${key}`] ?? null;
				return result;
			},
		},
	};
};

module.exports = { createMockPages };
