<template>
	<span class="hidden" />
</template>

<script>
	import {
		acceptDesktopLogin,
		installDesktopLogin,
	} from '@/actions/tmgr/auth';
	import {
		desktopAuthStatus,
		failDesktopSocialLogin,
	} from '@/composable/useDesktopSocialLogin';
	import store from '@/store';
	import {
		hasPendingDesktopAuth,
		isLatestDesktopAuth,
		parseAuthCallback,
		takePendingDesktopAuth,
	} from '@/utils/desktopAuth';
	import { createRecentUrlGuard } from '@/utils/desktopShortcuts';
	import {
		linkConfirmationNotice,
		linkConfirmationProvider,
	} from '@/utils/emailVerification';
	import { defineComponent, onBeforeUnmount, onMounted } from 'vue';
	import { useRouter } from 'vue-router';

	const isRecentDuplicateUrl = createRecentUrlGuard();

	const showMainWindow = async () => {
		const { getCurrentWindow } = await import('@tauri-apps/api/window');
		const win = getCurrentWindow();
		await win.show();
		await win.setFocus();
	};

	export default defineComponent({
		name: 'DesktopAuthLinks',
		setup() {
			const router = useRouter();
			let unlisten = null;

			const handle = async (urls) => {
				const url = (urls || []).find((u) => parseAuthCallback(u));
				if (!url || isRecentDuplicateUrl(url)) return;
				const link = parseAuthCallback(url);
				if ('error' in link) {
					// Uncorrelated (no state): report it, but keep the attempt redeemable.
					if (!hasPendingDesktopAuth()) return;
					await showMainWindow();
					const provider = linkConfirmationProvider(link.error);
					failDesktopSocialLogin(
						provider
							? linkConfirmationNotice(provider)
							: 'Sign-in was not completed. Please try again.',
					);
					return;
				}
				const pending = takePendingDesktopAuth(link.state);
				if (!pending) return;
				desktopAuthStatus.value = 'completing';
				await showMainWindow();
				let response;
				try {
					response = await acceptDesktopLogin(link.code, pending.verifier);
				} catch {
					if (isLatestDesktopAuth(pending.state) && !store.getters.isLoggedIn) {
						failDesktopSocialLogin(
							'Sign-in link expired or was already used. Please try again.',
						);
					}
					return;
				}
				// A newer attempt or another login took over while this one was in flight.
				if (!isLatestDesktopAuth(pending.state) || store.getters.isLoggedIn) return;
				installDesktopLogin(response);
				desktopAuthStatus.value = 'idle';
				const invitation = localStorage.getItem('workspace.invitation');
				await router.push(
					invitation
						? { name: 'WorkspaceInvitation', params: { token: invitation } }
						: '/',
				);
				store.commit('rerenderApp');
			};

			onMounted(async () => {
				const deepLink = await import('@tauri-apps/plugin-deep-link');
				unlisten = await deepLink.onOpenUrl(handle);
				handle(await deepLink.getCurrent());
			});

			onBeforeUnmount(() => unlisten?.());
		},
	});
</script>
