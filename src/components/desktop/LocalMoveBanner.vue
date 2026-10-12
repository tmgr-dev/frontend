<template>
	<div
		v-if="text"
		role="status"
		data-testid="local-copied-banner"
		class="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-emerald-300 bg-emerald-100 px-4 py-2 text-sm text-emerald-900 dark:border-emerald-500/30 dark:bg-emerald-500/10 dark:text-emerald-200"
	>
		<span class="min-w-0 flex-1 break-words">{{ text }}</span>
		<button
			type="button"
			class="rounded-md px-2 py-1 hover:bg-emerald-200 dark:hover:bg-emerald-500/20"
			aria-label="Dismiss"
			@click="dismiss"
		>
			<X class="h-4 w-4" />
		</button>
	</div>
</template>

<script lang="ts">
	import {
		bannerDismissedThisSession,
		copiedBannerText,
		dismissBannerForSession,
	} from '@/local/moveToCloud/banner';
	import { browserStorage, loadPersisted } from '@/local/moveToCloud/model';
	import { getMover } from '@/local/moveToCloud/runtime';
	import { activeLocalWorkspace } from '@/local/runtime';
	import { isDesktopApp } from '@/utils/desktop';
	import { X } from 'lucide-vue-next';
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		ref,
	} from 'vue';
	import { useStore } from 'vuex';

	export default defineComponent({
		name: 'LocalMoveBanner',
		components: { X },
		setup() {
			const store = useStore();
			const tick = ref(0);

			const code = computed(() => {
				void store.state.user?.settings;
				return activeLocalWorkspace()?.code ?? null;
			});
			const text = computed(() => {
				void tick.value;
				if (!isDesktopApp() || !code.value) return '';
				if (bannerDismissedThisSession(code.value)) return '';
				const { published } = loadPersisted(browserStorage, code.value);
				return published ? copiedBannerText(published) : '';
			});

			let unsubscribe: (() => void) | null = null;
			onMounted(() => {
				unsubscribe = getMover().subscribe(() => tick.value++);
			});
			onBeforeUnmount(() => unsubscribe?.());

			const dismiss = () => {
				if (code.value) dismissBannerForSession(code.value);
				tick.value++;
			};

			return { text, dismiss };
		},
	});
</script>
