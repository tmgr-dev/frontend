import { createRouter, createWebHistory } from 'vue-router';
import store from '../store';
import routes from './routes';

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

router.beforeEach((to, from, next) => {
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

		return next();
	}
	return next();
});

export default router;
