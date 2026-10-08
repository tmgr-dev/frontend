import $axios from '@/plugins/axios';
import type {
	GraphMap,
	GraphOrphans,
	GraphPath,
	GraphRanking,
	GraphResult,
} from '@/types/graph';
import { requestCache } from '@/utils/requestCache';
import {
	buildListParams,
	buildMapParams,
	buildPathParams,
	buildRelatedParams,
	type GraphListParams,
	type GraphMapParams,
	type GraphPathParams,
	type GraphRelatedParams,
} from './graphParams';

const GRAPH_TTL = 30000;

const unwrap = async <T>(request: Promise<any>): Promise<T> => {
	const {
		data: { data },
	} = await request;
	return data;
};

const cached = <T>(path: string, params: Record<string, unknown>) =>
	requestCache.getOrFetch<T>(
		`graph-${path}-${JSON.stringify(params)}`,
		() => unwrap<T>($axios.get(`graph/${path}`, { params })),
		{ ttl: GRAPH_TTL },
	);

export const getGraphRelated = (p: GraphRelatedParams): Promise<GraphResult> =>
	cached<GraphResult>('related', buildRelatedParams(p));

export const getGraphPath = (p: GraphPathParams): Promise<GraphPath> =>
	cached<GraphPath>('path', buildPathParams(p));

export const getGraphHubs = (p: GraphListParams): Promise<GraphRanking> =>
	cached<GraphRanking>('hubs', buildListParams(p));

export const getGraphOrphans = (p: GraphListParams): Promise<GraphOrphans> =>
	cached<GraphOrphans>('orphans', buildListParams(p));

export const getGraphMap = (p: GraphMapParams): Promise<GraphMap> =>
	cached<GraphMap>('map', buildMapParams(p));
