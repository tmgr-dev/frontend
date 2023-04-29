const TaskForm = () => import('src/pages/TaskForm.vue');
const TasksList = () => import('src/pages/TasksList.vue');
const ProjectCategoryList = () => import('src/pages/ProjectCategoryList.vue');
const ProjectCategoryForm = () => import('src/pages/ProjectCategoryForm.vue');

const routes = [
	{
		path: '/login',
		component: () => import('src/pages/auth/Login.vue'),
		name: 'Login',
		meta: {
			allowedGuests: true,
			transitionName: 'fade-fast',
		},
	},
	{
		path: '/register',
		component: () => import('src/pages/auth/Register.vue'),
		name: 'Register',
		meta: {
			allowedGuests: true,
			transitionName: 'fade-fast',
		},
	},
	{
		path: '/password/forget',
		component: () => import('src/pages/auth/ForgetPassword.vue'),
		name: 'ForgetPassword',
		meta: {
			allowedGuests: true,
			transitionName: 'fade-fast',
		},
	},
	{
		path: '/password/reset',
		component: () => import('src/pages/auth/NewPassword.vue'),
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
		component: TasksList,
		name: 'CurrentTasksList',
	},
	{
		path: '/archive',
		component: TasksList,
		meta: {
			status: 'done',
			transitionName: 'slide',
			navbarHidden: true,
		},
		name: 'ArchiveTasksList',
	},
	{
		path: '/board',
		component: () => import('src/pages/Board.vue'),
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
		component: () => import('src/pages/Settings.vue'),
		meta: {
			transitionName: 'fade-fast',
			navbarHidden: true,
		},
		name: 'Settings',
	},
	{
		path: '/profile',
		component: () => import('src/pages/Profile.vue'),
		meta: {
			transitionName: 'fade-fast',
			navbarHidden: true,
		},
		name: 'Profile',
	},
	{
		path: '/workspaces/invitations/:token',
		component: () => import('src/pages/WorkspaceInvitation.vue'),
		meta: {
			transitionName: 'fade-fast',
			navbarHidden: true,
		},
		name: 'WorkspaceInvitation',
	},
	{
		path: '/stats',
		component: () => import('src/pages/Stats.vue'),
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
		component: () => import('src/pages/PushNotificationsEnableGuide.vue'),
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
		component: () => import('src/pages/Error404.vue'),
	},
];

export default routes;
