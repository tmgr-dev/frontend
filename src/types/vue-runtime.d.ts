import '@vue/runtime-core';
import type { RouteLocationNormalizedLoaded, Router } from 'vue-router';

declare module '@vue/runtime-core' {
	export interface ComponentCustomProperties {
		$router: Router;
		$route: RouteLocationNormalizedLoaded;
		showAlert(title?: string, description?: string): void;
	}
}
