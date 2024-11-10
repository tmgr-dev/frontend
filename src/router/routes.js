//const TaskForm = () => import('@/pages/TaskForm.vue');
const TaskForm = () => import('@/pages/NewForm.vue');
const TasksListPage = () => import('@/pages/TasksListPage.vue');
const ProjectCategoryList = () => import('@/pages/ProjectCategoryList.vue');
const ProjectCategoryForm = () => import('@/pages/ProjectCategoryForm.vue');

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
	{
		path: '/',
		meta: {
			transitionName: 'slide',
			navbarHidden: true,
		},
		component: TasksListPage,
		name: 'CurrentTasksList',
	},
	{
		path: '/archive',
		component: TasksListPage,
		meta: {
			status: 'done',
			transitionName: 'slide',
			navbarHidden: true,
		},
		name: 'ArchiveTasksList',
	},
	{
		path: '/board',
		component: () => import('@/pages/Board.vue'),
		meta: {
			transitionName: 'fade-fast',
			navbarHidden: true,
		},
		name: 'Board',
	},
	{
		path: '/projects-categories',
		meta: {
			transitionName: 'slide',
			navbarHidden: true,
		},
		component: ProjectCategoryList,
		name: 'ProjectCategoryList',
	},
	{
		path: '/projects-categories/create',
		meta: {
			transitionName: 'slide',
			navbarHidden: true,
		},
		component: ProjectCategoryForm,
		name: 'ProjectCategoryCreate',
	},
	{
		path: '/projects-categories/:id',
		meta: {
			transitionName: 'slide',
			navbarHidden: true,
		},
		component: ProjectCategoryForm,
		name: 'ProjectCategoryEdit',
	},
	{
		path: '/projects-categories/status/:status?',
		meta: {
			transitionName: 'slide',
			navbarHidden: true,
		},
		component: ProjectCategoryList,
		name: 'ProjectCategoryChildrenListWithStatus',
	},
	{
		path: '/projects-categories/:id/children/:status?',
		meta: {
			transitionName: 'slide',
			navbarHidden: true,
		},
		component: ProjectCategoryList,
		name: 'ProjectCategoryChildrenList',
	},
	{
		path: '/projects-categories/:project_category_id/create',
		meta: {
			transitionName: 'slide',
			navbarHidden: true,
		},
		component: ProjectCategoryForm,
		name: 'ProjectCategoryCreateInCategory',
	},
	/* @todo investigate why we don't have this page (404) */
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
			transitionName: 'fade-fast',
			navbarHidden: true,
		},
		name: 'Settings',
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
	// Always leave this as last one,
	// but you can also remove it
	{
		path: '/:catchAll(.*)*',
		name: 'NotFound',
		component: () => import('@/pages/Error404.vue'),
	},
	{
		path: '/daily-routines',
		component: () => import('@/pages/DailyRoutine.vue'),
		meta: {
			transitionName: 'fade-fast',
			navbarHidden: true,
		},
		name: 'Daily routines',
	},
];

export default routes;
