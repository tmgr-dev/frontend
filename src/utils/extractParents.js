export default function extractParents(category, parents = []) {
	if (!category.parent_category) {
		return parents.reverse();
	}
	parents.push(category.parent_category);
	return extractParents(category.parent_category, parents);
}
