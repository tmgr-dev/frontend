import { gatedPage } from '@/router/moduleGate';
import store from '@/store';
import { SETTINGS_NAV_COLUMN_QUERY } from '@/utils/settingsNav';
import { boardRedirect, isSimpleMode } from '@/utils/simpleStatus';
//const TaskForm = () => import('@/pages/TaskForm.vue');
const TaskForm = () => import('@/pages/TaskFormWrapper.vue');
const TasksListPage = () => import('@/pages/TasksListPage.vue');
const CategoryPage = () => import('@/pages/CategoryPage.vue');
const categorySettings = (id) => (to) => ({
	name: 'WorkspaceCategoryChildren',
	params: { workspace_code: to.params.workspace_code, id: id(to) },
	query: { ...to.query, tab: 'settings' },
});
const categoriesQueryRedirect = (to) => {
	const { category, create, ...query } = to.query;
	const workspace_code = to.params.workspace_code;
	if (create != null && create !== '')
		return create === 'top'
			? { name: 'WorkspaceCategoryCreate', params: { workspace_code }, query }
			: {
					name: 'WorkspaceCategoryCreateInCategory',
					params: { workspace_code, category_id: String(create) },
					query,
				};
	if (category != null && category !== '')
		return {
			name: 'WorkspaceCategoryChildren',
			params: { workspace_code, id: String(category) },
			query: { ...query, tab: 'settings' },
		};
	return true;
};
const DashboardPage = () => import('@/pages/DashboardPage.vue');

// Helper function to get current workspace
const getCurrentWorkspaceCode = () => {
	// This function will be called at runtime
	const store = window.app?.$store;
	if (!store) return null;

	const currentWorkspaceId = store.state.user?.settings?.find(
		(setting) => setting.key === 'current_workspace',
	)?.value;

	const currentWorkspace = store.state.workspaces?.find(
		(workspace) => Number(workspace.id) === Number(currentWorkspaceId),
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

const simpleModeBoardGuard = (to) =>
	boardRedirect(
		(key) => store.getters['featureToggles/isFeatureEnabled'](key),
		to.params.workspace_code || getCurrentWorkspaceCode(),
	) ?? true;

const routes = [
	{
		path: '/welcome',
		component: () => import('@/pages/desktop/Welcome.vue'),
		name: 'Welcome',
		meta: {
			allowedGuests: true,
			transitionName: 'fade-fast',
		},
	},
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
			// A stale token in localStorage must not swallow the OAuth callback:
			// without this flag the guard bounces /login/github?code=... to the
			// app before the code is exchanged (classic "logged in but expired").
			notOnlyForLoggedUsers: true,
			transitionName: 'fade-fast',
		},
	},
	{
		path: '/desktop-auth/return',
		component: () => import('@/pages/auth/DesktopAuthReturn.vue'),
		name: 'DesktopAuthReturn',
		meta: {
			allowedGuests: true,
			notOnlyForLoggedUsers: true,
			transitionName: 'fade-fast',
		},
	},
	{
		path: '/desktop-auth/telegram',
		component: () => import('@/pages/auth/DesktopAuthTelegram.vue'),
		name: 'DesktopAuthTelegram',
		meta: {
			allowedGuests: true,
			notOnlyForLoggedUsers: true,
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
	{
		path: '/email/verify',
		component: () => import('@/pages/auth/VerifyEmail.vue'),
		name: 'VerifyEmail',
		meta: {
			returnAfterLogin: true,
			transitionName: 'fade-fast',
		},
	},
	{
		path: '/device',
		component: () => import('@/pages/auth/DeviceLogin.vue'),
		name: 'DeviceLogin',
		meta: {
			returnAfterLogin: true,
			transitionName: 'fade-fast',
		},
	},
	{
		path: '/auth/link-confirm',
		component: () => import('@/pages/auth/LinkConfirm.vue'),
		name: 'LinkConfirm',
		meta: {
			returnAfterLogin: true,
			transitionName: 'fade-fast',
		},
	},
	{
		path: '/',
		redirect: (to) => ({ path: '/app', query: to.query, hash: to.hash }),
	},
	{
		path: '/app',
		component: DashboardPage,
		meta: {
			title: 'Dashboard',
			transitionName: 'slide',
			navbarHidden: true,
			requiresAuth: true,
		},
		name: 'Dashboard',
		beforeEnter: (to) => {
			// Try to redirect to workspace path if possible
			const store = window.app?.$store;
			if (store && store.getters.canUseApp) {
				const currentWorkspaceId = store.state.user?.settings?.find(
					(setting) => setting.key === 'current_workspace',
				)?.value;

				const currentWorkspace = store.state.workspaces?.find(
					(workspace) => Number(workspace.id) === Number(currentWorkspaceId),
				);

				if (currentWorkspace?.code) {
					return {
						path: `/${currentWorkspace.code}/dashboard`,
						query: to.query,
					};
				}
			}
		},
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
		redirect: (to) => {
			// Get current workspace from store
			const store = window.app?.$store;
			if (!store || !store.getters.canUseApp) {
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
				query: to.query,
			});
		},
	},
	// Legacy path - redirect to new path for create category
	{
		path: '/projects-categories/create',
		redirect: (to) => {
			const workspaceCode = getCurrentWorkspaceCode();
			if (!workspaceCode) return '/';

			return {
				path: `/${workspaceCode}/categories/create`,
				query: to.query,
			};
		},
	},
	// Legacy path - redirect to new path for category edit
	{
		path: '/projects-categories/:id',
		redirect: (to) => {
			const workspaceCode = getCurrentWorkspaceCode();
			if (!workspaceCode) return '/';

			return {
				path: `/${workspaceCode}/categories/${to.params.id}`,
				query: to.query,
			};
		},
	},
	// Legacy path - redirect to new path for categories with status
	{
		path: '/projects-categories/status/:status?',
		redirect: (to) => {
			const workspaceCode = getCurrentWorkspaceCode();
			if (!workspaceCode) return '/';

			return {
				path: `/${workspaceCode}/categories/status/${to.params.status || ''}`,
				query: to.query,
			};
		},
	},
	// Legacy path - redirect to new path for category children
	{
		path: '/projects-categories/:id/children/:status?',
		redirect: (to) => {
			const workspaceCode = getCurrentWorkspaceCode();
			if (!workspaceCode) return '/';

			return {
				path: `/${workspaceCode}/categories/${to.params.id}/children/${
					to.params.status || ''
				}`,
				query: to.query,
			};
		},
	},
	// Legacy path - redirect to new path for create in category
	{
		path: '/projects-categories/:project_category_id/create',
		redirect: (to) => {
			const workspaceCode = getCurrentWorkspaceCode();
			if (!workspaceCode) return '/';

			return {
				path: `/${workspaceCode}/categories/${to.params.project_category_id}/create`,
				query: to.query,
			};
		},
	},
	// New URL structure for archive page
	{
		path: '/:workspace_code/files',
		component: gatedPage(
			'task.files',
			'Files',
			() => import('@/pages/WorkspaceFilesPage.vue'),
		),
		meta: {
			title: 'Files',
			transitionName: 'slide',
			navbarHidden: true,
		},
		name: 'WorkspaceFiles',
	},
	{
		path: '/welcome-setup',
		component: () => import('@/pages/onboarding/WelcomeSetup.vue'),
		meta: {
			title: 'Welcome',
			transitionName: 'fade-fast',
			navbarHidden: true,
		},
		name: 'WelcomeSetup',
	},
	{
		path: '/welcome-setup/agent',
		component: () => import('@/pages/onboarding/ConnectAgent.vue'),
		meta: {
			title: 'Connect your agent',
			transitionName: 'fade-fast',
			navbarHidden: true,
		},
		name: 'WelcomeSetupAgent',
	},
	{
		path: '/:workspace_code/getting-started',
		component: () => import('@/pages/GettingStarted.vue'),
		meta: {
			title: 'Getting started',
			transitionName: 'slide',
			navbarHidden: true,
		},
		name: 'WorkspaceGettingStarted',
	},
	{
		path: '/:workspace_code/plugins/:pluginId/:viewId',
		component: () => import('@/pages/PluginPage.vue'),
		meta: { title: 'Plugin', transitionName: 'slide', navbarHidden: true },
		name: 'WorkspacePluginPage',
	},
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
		redirect: (to) => {
			// Get current workspace from store
			const store = window.app?.$store;
			if (!store || !store.getters.canUseApp) {
				// If user is not logged in, show login page
				return safeRedirect(to, '/login');
			}

			// Get current workspace ID
			const currentWorkspaceId = store.state.user?.settings?.find(
				(setting) => setting.key === 'current_workspace',
			)?.value;

			// Find the workspace by ID
			const currentWorkspace = store.state.workspaces?.find(
				(workspace) => Number(workspace.id) === Number(currentWorkspaceId),
			);

			// If we have a workspace code, redirect to workspace board
			if (currentWorkspace?.code) {
				const target = isSimpleMode(
					store.getters['featureToggles/isFeatureEnabled'],
				)
					? 'list'
					: 'board';
				return safeRedirect(to, `/${currentWorkspace.code}/${target}`);
			}

			// If no workspace code is available, go to fallback board route
			return safeRedirect(to, '/fallback-board');
		},
		name: 'Board',
	},
	// Fallback route for when workspace code is not available
	{
		path: '/fallback-board',
		beforeEnter: simpleModeBoardGuard,
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
		path: '/quick-add',
		component: () => import('@/pages/QuickAdd.vue'),
		meta: { title: 'Quick add' },
		name: 'QuickAdd',
	},
	{
		path: '/settings',
		component: () => import('@/pages/Settings.vue'),
		beforeEnter: (to) =>
			!to.query.tab || to.query.tab === 'workspace'
				? { path: '/settings/workspaces', replace: true }
				: true,
		meta: {
			title: 'Settings',
			transitionName: 'fade-fast',
			navbarHidden: true,
			settingsLayout: true,
		},
		name: 'Settings',
	},
	{
		path: '/settings/sections',
		component: () => import('@/components/settings/SettingsSections.vue'),
		beforeEnter: () =>
			!window.matchMedia?.(SETTINGS_NAV_COLUMN_QUERY)?.matches
				? true
				: '/settings/workspaces',
		meta: {
			title: 'Settings',
			transitionName: 'fade-fast',
			navbarHidden: true,
			settingsLayout: true,
		},
		name: 'SettingsSections',
	},
	{
		path: '/settings/workspaces',
		component: () => import('@/pages/Settings/Workspaces.vue'),
		meta: {
			title: 'Workspace settings',
			transitionName: 'fade-fast',
			navbarHidden: true,
			settingsLayout: true,
		},
		name: 'WorkspaceSettings',
	},
	{
		path: '/settings/modules',
		component: () => import('@/pages/Settings/Modules.vue'),
		meta: {
			title: 'Modules',
			transitionName: 'fade-fast',
			navbarHidden: true,
			settingsLayout: true,
		},
		name: 'ModuleSettings',
	},
	{
		path: '/settings/features',
		redirect: '/settings/modules',
	},
	{
		path: '/settings/personas',
		component: gatedPage(
			'personas',
			'Personas',
			() => import('@/pages/Settings/Personas.vue'),
		),
		meta: {
			title: 'Personas',
			transitionName: 'fade-fast',
			navbarHidden: true,
			settingsLayout: true,
		},
		name: 'PersonaSettings',
	},
	{
		path: '/settings/agent-notifications',
		component: gatedPage(
			'alerts',
			'Agent alerts',
			() => import('@/pages/Settings/AgentNotifications.vue'),
		),
		meta: {
			title: 'Agent alerts',
			transitionName: 'fade-fast',
			navbarHidden: true,
			settingsLayout: true,
		},
		name: 'AgentNotificationSettings',
	},
	{
		path: '/settings/plugins',
		component: () => import('@/pages/Settings/Plugins.vue'),
		meta: {
			title: 'Plugins',
			transitionName: 'fade-fast',
			navbarHidden: true,
			settingsLayout: true,
		},
		name: 'PluginSettings',
	},
	{
		path: '/settings/plugins/help',
		component: () => import('@/pages/Settings/PluginsHelp.vue'),
		meta: {
			title: 'How plugins work',
			transitionName: 'fade-fast',
			navbarHidden: true,
			settingsLayout: true,
		},
		name: 'PluginSettingsHelp',
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
		path: '/ai-usage',
		component: () => import('@/pages/AiUsage.vue'),
		meta: {
			transitionName: 'fade-fast',
			navbarHidden: false,
			allowedGuests: true,
			notOnlyForLoggedUsers: true,
		},
		name: 'AiUsage',
		beforeEnter: async () => (await import('@/usage/guard')).desktopOnly(),
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
	{
		path: '/privacy',
		component: () => import('@/pages/Privacy.vue'),
		meta: {
			title: 'Privacy policy',
			transitionName: 'fade-fast',
			navbarHidden: false,
			allowedGuests: true,
			notOnlyForLoggedUsers: true,
		},
		name: 'Privacy',
	},
	{
		path: '/docs/agents',
		component: () => import('@/pages/AgentsGuide.vue'),
		meta: {
			title: 'AI agents setup',
			transitionName: 'fade-fast',
			navbarHidden: false,
			allowedGuests: true,
			notOnlyForLoggedUsers: true,
		},
		name: 'AgentsGuide',
	},

	// New URL structure routes
	// CHANGES: Added keepAlive meta for cached views
	{
		path: '/:workspace_code/dashboard',
		component: DashboardPage,
		meta: {
			title: 'Workspace Dashboard',
			transitionName: 'slide',
			navbarHidden: true,
			keepAlive: true,
		},
		name: 'WorkspaceDashboard',
	},
	{
		path: '/:workspace_code/team/:user_id',
		component: () => import('@/pages/MemberPage.vue'),
		meta: {
			title: 'Workspace Member',
			transitionName: 'slide',
			navbarHidden: true,
		},
		name: 'WorkspaceMember',
	},
	{
		path: '/:workspace_code/notifications',
		component: () => import('@/pages/NotificationsPage.vue'),
		meta: {
			title: 'Notifications',
			transitionName: 'slide',
			navbarHidden: true,
		},
		name: 'Notifications',
	},
	{
		path: '/:workspace_code/list',
		component: TasksListPage,
		meta: {
			title: 'Task List',
			transitionName: 'slide',
			navbarHidden: true,
			keepAlive: true,
		},
		name: 'WorkspaceTasksList',
	},
	{
		path: '/:workspace_code/board',
		beforeEnter: simpleModeBoardGuard,
		component: () => import('@/pages/Board.vue'),
		meta: {
			title: 'Board',
			transitionName: 'fade-fast',
			navbarHidden: true,
		},
		name: 'WorkspaceBoard',
	},
	{
		path: '/:workspace_code/map',
		component: () => import('@/pages/WorkspaceMap.vue'),
		meta: {
			title: 'Map',
			transitionName: 'fade-fast',
			navbarHidden: true,
		},
		name: 'WorkspaceMap',
	},
	{
		path: '/:workspace_code/task-window/:id',
		component: () => import('@/pages/TaskWindow.vue'),
		meta: { navbarHidden: true },
		name: 'TaskWindow',
	},
	{
		path: '/:workspace_code/page-window/:slug',
		component: () => import('@/pages/PageWindow.vue'),
		meta: { navbarHidden: true },
		name: 'PageWindow',
	},
	{
		path: '/:workspace_code/pages',
		component: () => import('@/pages/PagesLayout.vue'),
		meta: {
			title: 'Pages',
			transitionName: 'fade-fast',
			navbarHidden: true,
			pages: true,
		},
		children: [
			{
				path: '',
				component: () => import('@/pages/PagesIndex.vue'),
				name: 'WorkspacePages',
			},
			{
				path: '_/archive',
				component: () => import('@/pages/PagesTrash.vue'),
				name: 'WorkspacePagesTrash',
			},
			{
				path: '_/trash',
				redirect: (to) => ({
					name: 'WorkspacePagesTrash',
					params: to.params,
					query: to.query,
				}),
			},
			{
				path: ':slug',
				component: () => import('@/pages/PageView.vue'),
				name: 'WorkspacePage',
			},
			{
				path: ':slug/versions',
				component: () => import('@/pages/PageVersions.vue'),
				name: 'WorkspacePageVersions',
			},
		],
	},
	{
		path: '/:workspace_code/:category_code',
		component: CategoryPage,
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
		component: CategoryPage,
		meta: {
			title: 'Categories',
			transitionName: 'slide',
			navbarHidden: true,
		},
		name: 'WorkspaceCategories',
		beforeEnter: categoriesQueryRedirect,
	},
	{
		path: '/:workspace_code/categories/create',
		component: CategoryPage,
		meta: {
			title: 'New category',
			transitionName: 'slide',
			navbarHidden: true,
		},
		name: 'WorkspaceCategoryCreate',
	},
	{
		path: '/:workspace_code/categories/:id',
		redirect: categorySettings((to) => to.params.id),
		name: 'WorkspaceCategoryEdit',
	},
	{
		path: '/:workspace_code/categories/status/:status?',
		redirect: (to) => ({
			name: 'WorkspaceCategories',
			params: { workspace_code: to.params.workspace_code },
			query: to.query,
		}),
		name: 'WorkspaceCategoryListWithStatus',
	},
	{
		path: '/:workspace_code/categories/:id/children/:status?',
		meta: {
			title: 'Subcategories',
			transitionName: 'slide',
			navbarHidden: true,
		},
		component: CategoryPage,
		name: 'WorkspaceCategoryChildren',
	},
	{
		path: '/:workspace_code/categories/:category_id/create',
		component: CategoryPage,
		meta: {
			title: 'New category',
			transitionName: 'slide',
			navbarHidden: true,
		},
		name: 'WorkspaceCategoryCreateInCategory',
	},

	// Fallback route for categories when no workspace is available
	{
		path: '/fallback-categories',
		component: CategoryPage,
		meta: {
			title: 'Categories',
			transitionName: 'slide',
			navbarHidden: true,
			fallback: true,
		},
		name: 'FallbackCategoriesList',
	},

	// Note: All /projects-categories routes are defined as redirects earlier in the file
	// Removing duplicates to prevent conflicts and adding fallback routes instead

	// Fallback route for create category when redirect doesn't work
	{
		path: '/fallback-categories-create',
		redirect: (to) => ({
			name: 'FallbackCategoriesList',
			query: { ...to.query, create: 'top' },
		}),
		name: 'FallbackCategoryCreate',
	},

	// IMPORTANT: We're removing all component routes for /projects-categories/*
	// since they're already defined as redirects earlier in the file

	// Add missing ProjectCategoryChildrenList route
	{
		path: '/projects-categories/:id/children',
		component: CategoryPage,
		meta: {
			title: 'Category Children',
			transitionName: 'slide',
			navbarHidden: true,
		},
		name: 'ProjectCategoryChildrenList',
		beforeEnter: (to) => {
			// Try to redirect to workspace path if possible
			const store = window.app?.$store;
			if (store && store.getters.canUseApp) {
				const currentWorkspaceId = store.state.user?.settings?.find(
					(setting) => setting.key === 'current_workspace',
				)?.value;

				const currentWorkspace = store.state.workspaces?.find(
					(workspace) => Number(workspace.id) === Number(currentWorkspaceId),
				);

				if (currentWorkspace?.code) {
					return {
						path: `/${currentWorkspace.code}/categories/${to.params.id}/children`,
						query: to.query,
					};
				}
			}
		},
	},

	// Always leave this as last one,
	{
		path: '/:catchAll(.*)*',
		name: 'NotFound',
		component: () => import('@/pages/Error404.vue'),
	},
];

export default routes;
