import type { RouteLocationNormalized } from 'vue-router';

type Location = Pick<
	RouteLocationNormalized,
	'name' | 'path' | 'hash' | 'query'
>;

const pageQuery = ({
	tab: _tab,
	window: _window,
	...rest
}: Location['query']) =>
	JSON.stringify(Object.entries(rest).sort(([a], [b]) => a.localeCompare(b)));

export function scrollBehavior(
	to: Location,
	from: Location,
	_savedPosition?: unknown,
) {
	const sameMemberView =
		to.name === 'WorkspaceMember' &&
		from.name === 'WorkspaceMember' &&
		to.path === from.path &&
		to.hash === from.hash &&
		pageQuery(to.query) === pageQuery(from.query);
	return sameMemberView ? false : { left: 0, top: 0 };
}
