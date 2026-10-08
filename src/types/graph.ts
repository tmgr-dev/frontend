export type GraphNodeType =
	| 'task'
	| 'page'
	| 'user'
	| 'persona'
	| 'agent_run'
	| 'comment';

export interface GraphNode {
	id: string;
	type: GraphNodeType;
	ref_id: number | string | null;
	key: string | null;
	title: string;
	status: { name: string; type: string } | null;
	category_id: number | null;
	category: string | null;
	hop: number;
	weight: number;
	rel: string | null;
	meta: Record<string, unknown>;
}

export interface GraphEdge {
	id: string;
	from: string;
	to: string;
	type: string;
	label: string;
	weight: number;
	why: string;
}

export interface GraphResult {
	center: string;
	nodes: GraphNode[];
	edges: GraphEdge[];
	truncated: boolean;
	caps_hit: Record<string, number>;
}

export interface GraphPath {
	from: string;
	to: string;
	nodes: GraphNode[];
	edges: GraphEdge[];
	exhausted: boolean;
}

export interface GraphRankItem {
	node: GraphNode;
	degree: number;
	blocks: number;
}

export interface GraphRanking {
	hubs: GraphRankItem[];
	bottlenecks: GraphRankItem[];
}

export interface GraphOrphans {
	nodes: GraphNode[];
	total: number;
}

export interface GraphMapCategory {
	id: number | null;
	title: string;
	code: string | null;
	count: number;
}

export interface GraphBridge {
	from_category_id: number | null;
	to_category_id: number | null;
	edges: number;
}

export interface GraphMap {
	nodes: GraphNode[];
	edges: GraphEdge[];
	categories: GraphMapCategory[];
	insights: {
		hubs: GraphRankItem[];
		bottlenecks: GraphRankItem[];
		bridges: GraphBridge[];
		orphans: { total: number; ids: string[] };
	};
	truncated: boolean;
	total: number;
}
