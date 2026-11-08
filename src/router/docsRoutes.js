const DocsPage = () => import('@/pages/DocsPage.vue');

const meta = () => ({
	title: 'Docs',
	transitionName: 'fade-fast',
	navbarHidden: false,
	allowedGuests: true,
	notOnlyForLoggedUsers: true,
});

const docsRoutes = [
	{ path: '/help', redirect: '/docs' },
	{
		path: '/cli',
		redirect: (to) => ({ path: '/docs/cli', query: to.query, hash: to.hash }),
	},
	{
		path: '/docs/agents',
		component: DocsPage,
		meta: meta(),
		name: 'AgentsGuide',
	},
	{
		path: '/docs/:slug(.*)*',
		component: DocsPage,
		meta: meta(),
		name: 'Docs',
	},
];

export default docsRoutes;
