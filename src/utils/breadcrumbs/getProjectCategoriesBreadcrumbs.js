export default function getProjectCategoriesBreadcrumbs(parentCategories) {
	if (!parentCategories) {
		return [];
	}
	const result = [];
	parentCategories.map(item => result.push({
		label: item.title,
		to: `/projects-categories/${item ? item.id + '/children' : ''}`,
		payload: item
	}));
	return result;
}
