import type { LocalRouter } from '../router';
import { LocalHttpError, type LocalContext } from '../types';
import type { Group } from './data';
import { map } from './map';
import {
	ALL_GROUPS,
	hubs,
	orphans,
	path,
	related,
	type GraphOptions,
} from './service';

const intOf = (value: string | null): number | undefined => {
	if (value === null || value.trim() === '') return undefined;
	const n = Number(value);
	return Number.isFinite(n) ? n : undefined;
};

const requireParam = (value: string | null, name: string): string => {
	if (!value?.trim()) throw new LocalHttpError(422, `${name} is required`);
	return value;
};

const includeOf = (value: string | null): Group[] | undefined => {
	const picked = (value ?? '')
		.split(',')
		.map((s) => s.trim())
		.filter((s): s is Group => (ALL_GROUPS as string[]).includes(s));
	return picked.length ? picked : undefined;
};

export const graphDisabled = () =>
	new LocalHttpError(
		403,
		'The Graph feature is turned off for this workspace',
		'feature_disabled',
	);

export const addGraphRoutes = (
	router: LocalRouter,
	options: GraphOptions,
	isEnabled?: (ctx: LocalContext) => Promise<boolean>,
) => {
	const guarded = (route: (req: any) => any) => async (req: any) => {
		if (isEnabled && !(await isEnabled(req.ctx))) throw graphDisabled();
		return route(req);
	};
	return router
		.add(
			'GET',
			'graph/related',
			guarded(({ ctx, query }) =>
				related(
					ctx,
					{
						entity: requireParam(query.get('entity'), 'entity'),
						depth: intOf(query.get('depth')),
						include: includeOf(query.get('include')),
						limit: intOf(query.get('limit')),
					},
					options,
				),
			),
		)
		.add(
			'GET',
			'graph/path',
			guarded(({ ctx, query }) =>
				path(
					ctx,
					{
						from: requireParam(query.get('from'), 'from'),
						to: requireParam(query.get('to'), 'to'),
						max_depth: intOf(query.get('max_depth')),
					},
					options,
				),
			),
		)
		.add(
			'GET',
			'graph/hubs',
			guarded(({ ctx, query }) =>
				hubs(ctx, { limit: intOf(query.get('limit')) }, options),
			),
		)
		.add(
			'GET',
			'graph/orphans',
			guarded(({ ctx, query }) =>
				orphans(ctx, { limit: intOf(query.get('limit')) }, options),
			),
		)
		.add(
			'GET',
			'graph/map',
			guarded(({ ctx, query }) =>
				map(
					ctx,
					{
						from: query.get('from') ?? undefined,
						to: query.get('to') ?? undefined,
						limit: intOf(query.get('limit')),
					},
					options,
				),
			),
		);
};
