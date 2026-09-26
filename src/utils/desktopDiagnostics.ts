import type { App } from 'vue';
import type { Router } from 'vue-router';
import type { Store } from 'vuex';

type Level = 'info' | 'warn' | 'error';

const describe = (value: unknown): string => {
	if (value instanceof Error) return `${value.message}\n${value.stack || ''}`;
	try {
		return typeof value === 'string' ? value : JSON.stringify(value);
	} catch {
		return String(value);
	}
};

export function installDesktopDiagnostics(
	app: App,
	router: Router,
	store: Store<any>,
): void {
	const write = async (level: Level, message: string) => {
		try {
			const log = await import('@tauri-apps/plugin-log');
			await log[level](message);
		} catch {
			/* logging must never break the app */
		}
	};

	window.addEventListener('error', (event) =>
		write('error', `[window.error] ${describe(event.error || event.message)}`),
	);
	window.addEventListener('unhandledrejection', (event) =>
		write('error', `[unhandledrejection] ${describe(event.reason)}`),
	);
	document.addEventListener('securitypolicyviolation', (event) =>
		write(
			'warn',
			`[csp] ${event.violatedDirective} blocked=${event.blockedURI} source=${event.sourceFile}:${event.lineNumber}`,
		),
	);

	const previousErrorHandler = app.config.errorHandler;
	app.config.errorHandler = (err, instance, info) => {
		write(
			'error',
			`[vue.error] ${info} in <${instance?.$options?.name || 'anonymous'}>: ${describe(err)}`,
		);
		if (previousErrorHandler) previousErrorHandler(err, instance, info);
		else console.error(err);
	};

	router.onError((err, to) =>
		write('error', `[router.error] ${to?.fullPath}: ${describe(err)}`),
	);

	router.afterEach((to, from, failure) => {
		write(
			failure ? 'warn' : 'info',
			`[nav] ${from.fullPath} -> ${to.fullPath}${failure ? ` failure=${describe(failure)}` : ''}`,
		);
		const target = to.fullPath;
		setTimeout(() => {
			if (router.currentRoute.value.fullPath !== target) return;
			const main = document.querySelector('main.app-canvas');
			if (!main) return;
			const content = main?.lastElementChild as HTMLElement | null;
			const text = (content?.innerText || '').trim();
			if (text.length > 0 || !store.getters.isLoggedIn) return;
			const state = store.state as Record<string, any>;
			write(
				'error',
				`[blank] route=${target} name=${String(to.name)} matched=${to.matched
					.map((m) => m.path)
					.join(',')} sessionGeneration=${state.sessionGeneration} ` +
					`userLoaded=${state.featureToggles?.userLoaded} workspaceLoaded=${state.featureToggles?.workspaceLoaded} ` +
					`html=${(content?.outerHTML || 'null').slice(0, 1500)}`,
			);
		}, 3000);
	});
}
