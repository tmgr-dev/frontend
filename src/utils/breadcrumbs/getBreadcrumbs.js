import getProjectCategoriesBreadcrumbs from './getProjectCategoriesBreadcrumbs';

export default function getBreadcrumbs(parentCategories) {
	const items = [
		{
			label: 'Categories',
			to: '/projects-categories',
			payload: {
				id: null
			}
		}
	];
	return [...items, ...getProjectCategoriesBreadcrumbs(parentCategories)];
}
