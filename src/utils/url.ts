/**
 * Generates a task URL based on the new URL structure
 * @param taskId - The task ID
 * @param workspace - The workspace (optional)
 * @param category - The category (optional)
 * @returns The URL for the task
 */
export function generateTaskUrl(
    taskId: number | string,
    workspace?: { code: string } | null | undefined,
    category?: { code: string } | null | undefined
): string {
    // Validate taskId to prevent navigation to undefined URLs
    if (taskId === undefined || taskId === null) {
        console.error('Invalid taskId in generateTaskUrl:', taskId);
        return '/';
    }

    if (!workspace?.code) {
        return `/${taskId}`;
    }

    if (category?.code) {
        return `/${workspace.code}/${category.code}/${taskId}`;
    }

    return `/${workspace.code}/tasks/${taskId}`;
}

/**
 * Generates a workspace-specific URL
 * @param path - The path (list, board, etc.)
 * @param workspace - The workspace (optional)
 * @returns The URL for the workspace-specific page
 */
export function generateWorkspaceUrl(
    path: string,
    workspace?: { code: string } | null | undefined
): string {
    // Validate path to prevent navigation to undefined URLs
    if (!path) {
        console.error('Invalid path in generateWorkspaceUrl:', path);
        return '/';
    }

    if (!workspace?.code) {
        return `/${path}`;
    }

    return `/${workspace.code}/${path}`;
}

/**
 * Generates a category URL based on the new URL structure
 * @param categoryId - The category ID
 * @param workspace - The workspace (optional)
 * @returns The URL for the category
 */
export function generateCategoryUrl(
    categoryId: number | string,
    workspace?: { code: string } | null | undefined,
    action: string = 'children'
): string {
    // Validate categoryId to prevent navigation to undefined URLs
    if (categoryId === undefined || categoryId === null) {
        console.error('Invalid categoryId in generateCategoryUrl:', categoryId);
        return '/projects-categories';
    }

    if (!workspace?.code) {
        return `/projects-categories/${categoryId}/${action}`;
    }

    return `/${workspace.code}/categories/${categoryId}/${action}`;
} 