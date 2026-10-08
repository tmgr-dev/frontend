export type GraphInclude =
	| 'tasks'
	| 'pages'
	| 'people'
	| 'personas'
	| 'agent_runs'
	| 'comments';

export interface GraphRelatedParams {
	entity: string;
	depth?: 1 | 2;
	include?: GraphInclude[] | string;
	limit?: number;
	workspace_id?: number | string | null;
}

export interface GraphPathParams {
	from: string;
	to: string;
	max_depth?: number;
	workspace_id?: number | string | null;
}

export interface GraphListParams {
	workspace_id: number | string;
	limit?: number;
}

export interface GraphMapParams {
	workspace_id: number | string;
	from?: string | null;
	to?: string | null;
	limit?: number;
}

const compact = (
	params: Record<string, unknown>,
): Record<string, string | number> => {
	const out: Record<string, string | number> = {};
	for (const [key, value] of Object.entries(params)) {
		if (value === undefined || value === null || value === '') continue;
		out[key] = value as string | number;
	}
	return out;
};

export const buildRelatedParams = (p: GraphRelatedParams) =>
	compact({
		entity: p.entity,
		depth: p.depth,
		include: Array.isArray(p.include) ? p.include.join(',') : p.include,
		limit: p.limit,
		workspace_id: p.workspace_id,
	});

export const buildPathParams = (p: GraphPathParams) =>
	compact({
		from: p.from,
		to: p.to,
		max_depth: p.max_depth,
		workspace_id: p.workspace_id,
	});

export const buildListParams = (p: GraphListParams) =>
	compact({ workspace_id: p.workspace_id, limit: p.limit });

export const buildMapParams = (p: GraphMapParams) =>
	compact({
		workspace_id: p.workspace_id,
		from: p.from,
		to: p.to,
		limit: p.limit,
	});
