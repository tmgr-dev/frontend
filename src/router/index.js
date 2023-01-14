import { route } from 'quasar/wrappers';
import { createRouter, createWebHistory } from 'vue-router';
import routes from './routes';

/*
 * If not building with SSR mode, you can
 * directly export the Router instantiation;
 *
 * The function below can be async too; either use
 * async/await or return a Promise which resolves
 * with the Router instance.
 */
import store from '../store';

const router = route(function(/* { store, ssrContext } */) {
	const createHistory = createWebHistory;

	const Router = createRouter({
		scrollBehavior: () => ({ left: 0, top: 0 }),
		routes,

		// Leave this as is and make changes in quasar.conf.js instead!
		// quasar.conf.js -> build -> vueRouterMode
		// quasar.conf.js -> build -> publicPath
		history: createHistory(process.env.MODE === 'ssr' ? void 0 : process.env.VUE_ROUTER_BASE)
	});

	Router.beforeEach((to, from, next) => {
		store.getters.slideout.close();
		if (to.matched.some(record => record.meta.allowedGuests) && store.getters.isLoggedIn) {
			if (to.matched.some(record => record.meta.notOnlyForLoggedUsers)) {
				return next();
			}
			return next({ name: 'CurrentTasksList' });
		}
		if (to.matched.some(record => !record.meta.allowedGuests)) {
			if (!store.getters.isLoggedIn) {
				return next({ name: 'Login' });
			}
			return next();
		}
		return next();
	});

	return Router;
});

export default router;
