<template>
	<div
		v-if="visible"
		role="status"
		data-testid="desktop-app-bar"
		class="flex flex-wrap items-center justify-center gap-x-3 gap-y-1 border-b border-sky-300 bg-sky-100 px-4 py-1.5 text-sm text-sky-900 dark:border-sky-500/30 dark:bg-sky-500/10 dark:text-sky-200"
	>
		<span>You use TMGR Desktop.</span>
		<a
			:href="openHref"
			class="rounded-md border border-sky-400 bg-white px-2 py-0.5 font-medium hover:bg-sky-200 dark:border-sky-500/50 dark:bg-transparent dark:hover:bg-sky-500/20"
		>
			Open in TMGR Desktop
		</a>
		<button
			type="button"
			class="rounded-md px-2 py-1 hover:bg-sky-200 dark:hover:bg-sky-500/20"
			aria-label="Hide this bar until next sign-in"
			@click="dismiss"
		>
			<X class="h-4 w-4" />
		</button>
	</div>
</template>

<script lang="ts">
	import { isDesktopApp } from '@/utils/desktop';
	import {
		buildOpenUrl,
		DESKTOP_BAR_DISMISS_KEY,
		desktopBarVisible,
		parseOpenPath,
	} from '@/utils/openDesktopLink';
	import { X } from 'lucide-vue-next';
	import { computed, defineComponent, ref, watch } from 'vue';
	import { useRoute, useRouter } from 'vue-router';
	import { useStore } from 'vuex';

	const readDismissed = (): string | null => {
		try {
			return localStorage.getItem(DESKTOP_BAR_DISMISS_KEY);
		} catch {
			return null;
		}
	};

	const writeDismissed = (value: string) => {
		try {
			localStorage.setItem(DESKTOP_BAR_DISMISS_KEY, value);
		} catch {
			return;
		}
	};

	export default defineComponent({
		name: 'DesktopAppBar',
		components: { X },
		setup() {
			const store = useStore();
			const route = useRoute();
			const router = useRouter();
			const dismissedForUserId = ref<string | null>(readDismissed());

			const userId = computed(() => store.state.user?.id ?? null);

			watch([() => store.getters.isLoggedIn, userId], () => {
				dismissedForUserId.value = readDismissed();
			});

			const visible = computed(() =>
				desktopBarVisible({
					desktopSeenAt: store.state.user?.desktop_app_seen_at,
					now: Date.now(),
					isDesktopApp: isDesktopApp(),
					loggedIn: store.getters.isLoggedIn,
					userLoaded: !!store.state.user?.id,
					dismissedForUserId: dismissedForUserId.value,
					userId: userId.value,
					nav: navigator,
				}),
			);

			const openHref = computed(() =>
				buildOpenUrl(parseOpenPath(route.path, (path) => router.resolve(path))),
			);

			const dismiss = () => {
				if (userId.value == null) return;
				const value = String(userId.value);
				writeDismissed(value);
				dismissedForUserId.value = value;
			};

			return { visible, openHref, dismiss };
		},
	});
</script>
