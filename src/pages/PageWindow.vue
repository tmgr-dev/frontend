<template>
	<div class="flex h-screen flex-col overflow-y-auto bg-surface font-display text-ink">
		<div
			v-if="error"
			role="alert"
			class="m-auto max-w-sm p-6 text-center text-sm text-ink-muted"
		>
			<p>{{ error }}</p>
			<button
				type="button"
				class="mt-3 rounded-md border border-line bg-surface-sunken px-3 py-1.5 text-ink hover:bg-surface-hover"
				@click="closeWindow"
			>
				Close window
			</button>
		</div>
		<div v-else-if="!ready" class="m-auto text-sm text-ink-subtle">Loading...</div>
		<FeatureGate
			v-else
			feature-key="pages"
			title="Страницы"
			description="Заметки, контекст воркспейса и документация в дереве страниц, которые редактируют и люди, и агенты."
			:icon="FileText"
		>
			<template #preview>
				<PagesPreview />
			</template>
			<PageView />
		</FeatureGate>
	</div>
</template>

<script lang="ts">
	import {
		getUserFeatureToggles,
		getWorkspaceFeatureToggles,
	} from '@/actions/tmgr/featureToggles';
	import { getWorkspaces } from '@/actions/tmgr/workspaces';
	import { usePagesRealtime } from '@/composable/usePagesRealtime';
	import FeatureGate from '@/components/general/FeatureGate.vue';
	import PagesPreview from '@/components/previews/PagesPreview.vue';
	import PageView from '@/pages/PageView.vue';
	import store from '@/store';
	import { syncActiveLocalWorkspace } from '@/utils/localWorkspaceSync';
	import { bootstrapWindowWorkspace } from '@/utils/taskWindowBootstrap';
	import { installTaskWindowNavigationGuard } from '@/utils/taskWindowBridge';
	import { FileText } from 'lucide-vue-next';
	import { computed, defineComponent, onBeforeUnmount, ref, watch } from 'vue';
	import { useRoute, useRouter } from 'vue-router';

	export default defineComponent({
		name: 'PageWindow',
		components: { FeatureGate, PagesPreview, PageView },
		setup() {
			const route = useRoute();
			const router = useRouter();
			const ready = ref(false);
			const error = ref('');

			const closeWindow = async () => {
				try {
					const { getCurrentWindow } = await import('@tauri-apps/api/window');
					await getCurrentWindow().close();
				} catch {
					window.close();
				}
			};

			const removeGuard = installTaskWindowNavigationGuard(
				router,
				undefined,
				'PageWindow',
			);
			onBeforeUnmount(removeGuard);

			watch(
				() => store.getters.isLoggedIn,
				(loggedIn) => {
					if (!loggedIn) void closeWindow();
				},
			);

			const open = async () => {
				try {
					const workspace = await bootstrapWindowWorkspace(
						store,
						{
							getWorkspaces,
							syncActiveLocalWorkspace,
							getUserFeatureToggles,
							getWorkspaceFeatureToggles,
						},
						String(route.params.workspace_code),
					);
					if (!workspace) {
						error.value = 'This page could not be found.';
						return;
					}
					ready.value = true;
				} catch (e) {
					console.error('Error opening page window:', e);
					error.value = 'The page could not be opened. Close this window and try again.';
				}
			};
			void open();

			const workspaceId = computed(() => {
				const id = Number(store.getters.currentWorkspaceId);
				return Number.isFinite(id) && id !== 0 ? id : null;
			});
			usePagesRealtime(workspaceId, ready, () => undefined);

			return { ready, error, closeWindow, FileText };
		},
	});
</script>
