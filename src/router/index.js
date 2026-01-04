import { createRouter, createWebHistory } from 'vue-router';
import store from '../store';
import routes from './routes';
import { getUser } from '@/actions/tmgr/user';
import { getWorkspaces } from '@/actions/tmgr/workspaces';
import { getWorkspaceFeatureToggles } from '@/actions/tmgr/featureToggles';

/*
 * If not building with SSR mode, you can
 * directly export the Router instantiation;
 *
 * The function below can be async too; either use
 * async/await or return a Promise which resolves
 * with the Router instance.
 */
const router = createRouter({
	scrollBehavior: () => ({ left: 0, top: 0 }),
	routes,
	history: createWebHistory(
		import.meta.env.MODE === 'ssr' ? void 0 : import.meta.env.VUE_ROUTER_BASE,
	),
});

router.beforeEach(async (to, from, next) => {
	if (
		to.matched.some((record) => record.meta.allowedGuests) &&
		store.getters.isLoggedIn
	) {
		if (to.matched.some((record) => record.meta.notOnlyForLoggedUsers)) {
			return next();
		}

		return next({ name: 'CurrentTasksList' });
	}

	if (to.matched.some((record) => !record.meta.allowedGuests)) {
		if (!store.getters.isLoggedIn) {
			return next({ name: 'Login' });
		}

		// Handle root path redirect for logged-in users
		if (to.path === '/' && store.getters.isLoggedIn) {
			// Ensure user and workspaces are loaded after a hard reload
			if (!store.state.user?.id) {
				try {
					await getUser();
				} catch (e) {
					// if fails, send to login
					return next({ name: 'Login' });
				}
			}

			if (!store.state.workspaces || store.state.workspaces.length === 0) {
				try {
					const workspaces = await getWorkspaces();
					store.commit('setWorkspaces', workspaces);
				} catch (e) {
					// fall back to login if workspace fetch fails
					return next({ name: 'Login' });
				}
			}

			// Ensure user toggles are loaded (needed for default landing page)
			if (!store.state.featureToggles?.userLoaded) {
				await store.dispatch('featureToggles/loadUserToggles');
			}

			// Ensure workspace toggles are loaded
			const currentWorkspaceId = store.state.user?.settings?.find(
				s => s.key === 'current_workspace'
			)?.value;

			if (currentWorkspaceId && !store.state.featureToggles?.workspaceLoaded) {
				try {
					const data = await getWorkspaceFeatureToggles(currentWorkspaceId);
					store.commit('featureToggles/setWorkspaceToggles', data);
				} catch (e) {
					// continue with default list
				}
			}

			const landingPage = store.getters['featureToggles/getUserFeatureValue']('default_landing_page') || 'list';
			const workspace = store.state.workspaces?.find(w => w.id == currentWorkspaceId);
			const workspaceCode = workspace?.code;

			// Validate landing page against allowed features
			let finalLanding = landingPage;
			const allowedLanding = ['list'];
			if (store.state.featureToggles?.workspaceToggles?.board?.enabled) {
				allowedLanding.push('board');
			}
			if (store.state.featureToggles?.workspaceToggles?.dashboard?.enabled) {
				allowedLanding.push('dashboard');
			}
			if (!allowedLanding.includes(finalLanding)) {
				finalLanding = allowedLanding[0];
				// persist fix
				await store.dispatch('featureToggles/updateUserToggles', {
					default_landing_page: finalLanding,
				});
			}
			
			if (workspaceCode) {
				return next(`/${workspaceCode}/${finalLanding}`);
			}
			
			// fallback: first workspace to landing page
			if (store.state.workspaces?.[0]?.code) {
				return next(`/${store.state.workspaces[0].code}/${finalLanding}`);
			}
		}

		return next();
	}
	return next();
});

export default router;
