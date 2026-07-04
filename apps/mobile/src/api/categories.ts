import { api } from './client';

export type Category = {
	id: number;
	title: string;
	color?: string;
	project_category_id?: number | null;
	workspace_id?: number | null;
};

// Workspace project categories (drive the board category filter).
export async function getCategories(): Promise<Category[]> {
	const { data } = await api.get('project_categories?all');
	return data?.data ?? [];
}

export async function createCategory(payload: Partial<Category>): Promise<Category> {
	const { data } = await api.post('project_categories', payload);
	return data?.data ?? data;
}

export async function updateCategory(
	id: number,
	payload: Partial<Category>,
): Promise<Category> {
	const { data } = await api.put(`project_categories/${id}`, payload);
	return data?.data ?? data;
}

export async function deleteCategory(id: number): Promise<void> {
	await api.delete(`project_categories/${id}`);
}
