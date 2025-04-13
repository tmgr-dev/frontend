//const TaskForm = () => import('@/pages/TaskForm.vue');
const TaskForm = () => import('@/pages/TaskFormWrapper.vue');
const TasksListPage = () => import('@/pages/TasksListPage.vue');
const ProjectCategoryList = () => import('@/pages/ProjectCategoryList.vue');
const ProjectCategoryForm = () => import('@/pages/ProjectCategoryForm.vue');
const DashboardPage = () => import('@/pages/DashboardPage.vue');

// Helper function to get current workspace
const getCurrentWorkspaceCode = () => {
	// This function will be called at runtime
	const store = window.app?.$store;
	if (!store) return null;
	
	const currentWorkspaceId = store.state.user?.settings?.find(
		setting => setting.key === 'current_workspace'
	)?.value;
	
	const currentWorkspace = store.state.workspaces?.find(
		workspace => Number(workspace.id) === Number(currentWorkspaceId)
	);
	
	return currentWorkspace?.code;
};

// Add safeRedirect helper function at the top
const safeRedirect = (to, target) => {
	// Prevent infinite redirects by checking if the current path matches the target
	if (typeof target === 'string') {
		// For string targets, compare paths directly
		if (to.path === target || to.fullPath === target) {
			console.warn(`Prevented infinite redirect: ${to.path} → ${target}`);
			// Return a fallback based on the path type
			if (to.path.includes('/projects-categories')) {
				return '/fallback-categories';
			} else if (to.path.includes('/list')) {
				return '/fallback-list';
			} else if (to.path.includes('/board')) {
				return '/fallback-board';
			} else {
				return '/';
			}
		}
	} else if (typeof target === 'object' && target.path) {
		// For object targets, compare the path property
		if (to.path === target.path || to.fullPath === target.path) {
			console.warn(`Prevented infinite redirect: ${to.path} → ${target.path}`);
			// Return a fallback based on the path type
			if (target.path.includes('/categories')) {
				return '/fallback-categories';
			} else if (target.path.includes('/list')) {
				return '/fallback-list';
			} else if (target.path.includes('/board')) {
				return '/fallback-board';
			} else {
				return '/';
			}
		}
	}
	return target;
};

const routes = [
	{
		path: '/login',
		component: () => import('@/pages/auth/Login.vue'),
		name: 'Login',
		meta: {
			allowedGuests: true,
			transitionName: 'fade-fast',
		},
	},
	{
		path: '/login/:platform',
		component: () => import('@/pages/auth/SocialiteProxy.vue'),
		name: 'SocialiteProxy',
		meta: {
			allowedGuests: true,
			transitionName: 'fade-fast',
		},
	},
	{
		path: '/register',
		component: () => import('@/pages/auth/Register.vue'),
		name: 'Register',
		meta: {
			allowedGuests: true,
			transitionName: 'fade-fast',
		},
	},
	{
		path: '/password/forget',
		component: () => import('@/pages/auth/ForgetPassword.vue'),
		name: 'ForgetPassword',
		meta: {
			allowedGuests: true,
			transitionName: 'fade-fast',
		},
	},
	{
		path: '/password/reset',
		component: () => import('@/pages/auth/NewPassword.vue'),
		name: 'NewPassword',
		meta: {
			allowedGuests: true,
			transitionName: 'fade-fast',
		},
	},
	// Legacy routes - keep for backward compatibility
	{
		path: '/',
		component: DashboardPage,
		meta: {
			title: 'Dashboard',
			transitionName: 'slide',
			navbarHidden: true,
			requiresAuth: true,
		},
		name: 'Dashboard',
	},
	// Define a direct route for the list page (legacy approach)
	{
		path: '/list',
		component: TasksListPage,
		meta: {
			title: 'Current tasks',
			transitionName: 'slide',
			navbarHidden: true,
		},
		name: 'CurrentTasksList',
	},
	// Fallback route for when workspace code is not available
	{
		path: '/fallback-list',
		component: TasksListPage,
		meta: {
			title: 'Current tasks',
			transitionName: 'slide',
			navbarHidden: true,
		},
		name: 'FallbackTasksList',
	},
	{
		path: '/archive',
		component: TasksListPage,
		meta: {
			title: 'Archive',
			status: 'done',
			transitionName: 'slide',
			navbarHidden: true,
		},
		name: 'ArchiveTasksList',
	},
	// Legacy path - redirect to new workspace path for categories
	{
		path: '/projects-categories',
		redirect: to => {
			// Get current workspace from store
			const store = window.app?.$store;
			if (!store || !store.getters.isLoggedIn) {
				return safeRedirect(to, '/fallback-categories'); // Redirect to fallback route
			}
			
			// Get current workspace
			const workspaceCode = getCurrentWorkspaceCode();
			if (!workspaceCode) {
				return safeRedirect(to, '/fallback-categories'); // Redirect to fallback route
			}
			
			// Redirect to workspace-specific categories
			return safeRedirect(to, {
				path: `/${workspaceCode}/categories`,
				query: to.query
			});
		}
	},
	// Legacy path - redirect to new path for create category
	{
		path: '/projects-categories/create',
		redirect: to => {
			const workspaceCode = getCurrentWorkspaceCode();
			if (!workspaceCode) return '/';
			
			return {
				path: `/${workspaceCode}/categories/create`,
				query: to.query
			};
		},
	},
	// Legacy path - redirect to new path for category edit
	{
		path: '/projects-categories/:id',
		redirect: to => {
			const workspaceCode = getCurrentWorkspaceCode();
			if (!workspaceCode) return '/';
			
			return {
				path: `/${workspaceCode}/categories/${to.params.id}`,
				query: to.query
			};
		},
	},
	// Legacy path - redirect to new path for categories with status
	{
		path: '/projects-categories/status/:status?',
		redirect: to => {
			const workspaceCode = getCurrentWorkspaceCode();
			if (!workspaceCode) return '/';
			
			return {
				path: `/${workspaceCode}/categories/status/${to.params.status || ''}`,
				query: to.query
			};
		},
	},
	// Legacy path - redirect to new path for category children
	{
		path: '/projects-categories/:id/children/:status?',
		redirect: to => {
			const workspaceCode = getCurrentWorkspaceCode();
			if (!workspaceCode) return '/';
			
			return {
				path: `/${workspaceCode}/categories/${to.params.id}/children/${to.params.status || ''}`,
				query: to.query
			};
		},
	},
	// Legacy path - redirect to new path for create in category
	{
		path: '/projects-categories/:project_category_id/create',
		redirect: to => {
			const workspaceCode = getCurrentWorkspaceCode();
			if (!workspaceCode) return '/';
			
			return {
				path: `/${workspaceCode}/categories/${to.params.project_category_id}/create`,
				query: to.query
			};
		},
	},
	// New URL structure for archive page
	{
		path: '/:workspace_code/archive',
		component: TasksListPage,
		meta: {
			title: 'Archive',
			status: 'done',
			transitionName: 'slide',
			navbarHidden: true,
		},
		name: 'ArchiveTasksListWithWorkspace',
	},
	{
		path: '/board',
		redirect: to => {
			// Get current workspace from store
			const store = window.app?.$store;
			if (!store || !store.getters.isLoggedIn) {
				// If user is not logged in, show login page
				return safeRedirect(to, '/login');
			}
			
			// Get current workspace ID
			const currentWorkspaceId = store.state.user?.settings?.find(
				setting => setting.key === 'current_workspace'
			)?.value;
			
			// Find the workspace by ID
			const currentWorkspace = store.state.workspaces?.find(
				workspace => Number(workspace.id) === Number(currentWorkspaceId)
			);
			
			// If we have a workspace code, redirect to workspace board
			if (currentWorkspace?.code) {
				return safeRedirect(to, `/${currentWorkspace.code}/board`);
			}
			
			// If no workspace code is available, go to fallback board route
			return safeRedirect(to, '/fallback-board');
		},
		name: 'Board',
	},
	// Fallback route for when workspace code is not available
	{
		path: '/fallback-board',
		component: () => import('@/pages/Board.vue'),
		meta: {
			title: 'Board',
			transitionName: 'fade-fast',
			navbarHidden: true,
		},
		name: 'FallbackBoard',
	},
	// Legacy paths for routines
	{
		path: '/routines',
		component: () => import('@/pages/DailyRoutine.vue'),
		meta: {
			title: 'Daily routines',
			transitionName: 'fade-fast',
			navbarHidden: true,
		},
		name: 'DailyRoutines',
	},
	// Legacy path - redirect to new path
	{
		path: '/daily-routines',
		redirect: '/routines',
	},
	// Note: The main /projects-categories route is defined earlier in the file
	// Here we only define the sub-routes
	/* All /projects-categories routes are already defined as redirects earlier in this file.
	   Including duplicate route definitions will cause conflicts and infinite redirects.
	   The redundant component-based routes have been removed. */
	{
		path: '/project-categories/:project_category_id/tasks/create',
		meta: {
			transitionName: 'slide',
			navbarHidden: true,
		},
		component: TaskForm,
		name: 'TasksCreateWithProjectCategoryId',
	},
	{
		path: '/create',
		meta: {
			transitionName: 'slide',
			navbarHidden: true,
		},
		component: TaskForm,
		name: 'TasksCreate',
	},
	// Legacy task route - keep for backward compatibility
	{
		path: '/:id',
		meta: {
			transitionName: 'slide',
			navbarHidden: true,
		},
		component: TaskForm,
		name: 'TasksEdit',
	},
	{
		path: '/settings',
		component: () => import('@/pages/Settings.vue'),
		meta: {
			title: 'Settings',
			transitionName: 'fade-fast',
			navbarHidden: true,
		},
		name: 'Settings',
	},
	{
		path: '/settings/workspaces',
		component: () => import('@/pages/Settings/Workspaces.vue'),
		meta: {
			title: 'Workspace settings',
			transitionName: 'fade-fast',
			navbarHidden: true,
		},
		name: 'WorkspaceSettings',
	},
	{
		path: '/profile',
		component: () => import('@/pages/Profile.vue'),
		meta: {
			transitionName: 'fade-fast',
			navbarHidden: true,
		},
		name: 'Profile',
	},
	{
		path: '/workspaces/invitations/:token',
		component: () => import('@/pages/WorkspaceInvitation.vue'),
		meta: {
			transitionName: 'fade-fast',
			navbarHidden: true,
		},
		name: 'WorkspaceInvitation',
	},
	{
		path: '/stats',
		component: () => import('@/pages/Stats.vue'),
		meta: {
			transitionName: 'fade-fast',
			navbarHidden: false,
			allowedGuests: true,
			notOnlyForLoggedUsers: true,
		},
		name: 'Stats',
	},
	{
		path: '/push-notifications-enable-guide',
		component: () => import('@/pages/PushNotificationsEnableGuide.vue'),
		meta: {
			transitionName: 'fade-fast',
			navbarHidden: false,
			allowedGuests: true,
			notOnlyForLoggedUsers: true,
		},
		name: 'PushNotificationsEnableGuide',
	},
	
	// New URL structure routes
	{
		path: '/:workspace_code/list',
		component: TasksListPage,
		meta: {
			title: 'Task List',
			transitionName: 'slide',
			navbarHidden: true,
		},
		name: 'WorkspaceTasksList',
	},
	{
		path: '/:workspace_code/board',
		component: () => import('@/pages/Board.vue'),
		meta: {
			title: 'Board',
			transitionName: 'fade-fast',
			navbarHidden: true,
		},
		name: 'WorkspaceBoard',
	},
	{
		path: '/:workspace_code/:category_code',
		component: ProjectCategoryList,
		meta: {
			title: 'Category View',
			transitionName: 'slide',
			navbarHidden: true,
		},
		name: 'WorkspaceCategory',
	},
	{
		path: '/:workspace_code/:category_code/:task_number',
		component: TaskForm,
		meta: {
			transitionName: 'slide',
			navbarHidden: true,
		},
		name: 'WorkspaceCategoryTask',
	},
	{
		path: '/:workspace_code/tasks/:task_id',
		component: TaskForm,
		meta: {
			transitionName: 'slide',
			navbarHidden: true,
		},
		name: 'WorkspaceTask',
	},
	{
		path: '/:workspace_code/categories',
		component: ProjectCategoryList,
		meta: {
			title: 'Categories',
			transitionName: 'slide',
			navbarHidden: true,
		},
		name: 'WorkspaceCategories',
	},
	{
		path: '/:workspace_code/categories/create',
		meta: {
			title: 'Create category',
			transitionName: 'slide',
			navbarHidden: true,
		},
		component: ProjectCategoryForm,
		name: 'WorkspaceCategoryCreate',
	},
	{
		path: '/:workspace_code/categories/:id',
		meta: {
			title: 'Edit category',
			transitionName: 'slide',
			navbarHidden: true,
		},
		component: ProjectCategoryForm,
		name: 'WorkspaceCategoryEdit',
	},
	{
		path: '/:workspace_code/categories/status/:status?',
		meta: {
			title: 'Categories by status',
			transitionName: 'slide',
			navbarHidden: true,
		},
		component: ProjectCategoryList,
		name: 'WorkspaceCategoryListWithStatus',
	},
	{
		path: '/:workspace_code/categories/:id/children/:status?',
		meta: {
			title: 'Subcategories',
			transitionName: 'slide',
			navbarHidden: true,
		},
		component: ProjectCategoryList,
		name: 'WorkspaceCategoryChildren',
	},
	{
		path: '/:workspace_code/categories/:category_id/create',
		meta: {
			title: 'Create subcategory',
			transitionName: 'slide',
			navbarHidden: true,
		},
		component: ProjectCategoryForm,
		name: 'WorkspaceCategoryCreateInCategory',
	},
	
	// Fallback route for categories when no workspace is available
	{
		path: '/fallback-categories',
		component: ProjectCategoryList,
		meta: {
			title: 'Categories',
			transitionName: 'slide',
			navbarHidden: true,
			fallback: true
		},
		name: 'FallbackCategoriesList',
	},
	
	// Note: All /projects-categories routes are defined as redirects earlier in the file
	// Removing duplicates to prevent conflicts and adding fallback routes instead
	
	// Fallback route for create category when redirect doesn't work
	{
		path: '/fallback-categories-create',
		meta: {
			title: 'Create category',
			transitionName: 'slide',
			navbarHidden: true,
		},
		component: ProjectCategoryForm,
		name: 'FallbackCategoryCreate',
	},
	
	// IMPORTANT: We're removing all component routes for /projects-categories/*
	// since they're already defined as redirects earlier in the file
	
	// Add missing ProjectCategoryChildrenList route
	{
		path: '/projects-categories/:id/children',
		meta: {
			title: 'Category Children',
			transitionName: 'slide',
			navbarHidden: true,
		},
		component: ProjectCategoryList,
		name: 'ProjectCategoryChildrenList',
	},
	
	// Always leave this as last one,
	// but you can also remove it
	{
		path: '/:catchAll(.*)*',
		name: 'NotFound',
		component: () => import('@/pages/Error404.vue'),
	},
];

export default routes;
