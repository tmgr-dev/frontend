<template>
	<div class="flex h-screen flex-col bg-surface font-display text-ink">
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
		<NewForm v-else :is-modal="true" detached @close="closeWindow" />
	</div>
</template>

<script lang="ts">
	import { getWorkspaces } from '@/actions/tmgr/workspaces';
	import NewForm from '@/pages/NewForm.vue';
	import store from '@/store';
	import { syncActiveLocalWorkspace } from '@/utils/localWorkspaceSync';
	import { installTaskWindowNavigationGuard } from '@/utils/taskWindowBridge';
	import { defineComponent, onBeforeUnmount, ref, watch } from 'vue';
	import { useRoute, useRouter } from 'vue-router';

	export default defineComponent({
		name: 'TaskWindow',
		components: { NewForm },
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

			const removeGuard = installTaskWindowNavigationGuard(router);
			onBeforeUnmount(removeGuard);

			watch(
				() => store.getters.isLoggedIn,
				(loggedIn) => {
					if (!loggedIn) void closeWindow();
				},
			);

			const open = async () => {
				const taskId = Number(route.params.id);
				try {
					if (!store.state.workspaces?.length) {
						store.commit('setWorkspaces', await getWorkspaces());
					}
					const workspace = ((store.state.workspaces || []) as any[]).find(
						(w) => w.code === route.params.workspace_code,
					);
					if (!workspace || !Number.isFinite(taskId) || taskId <= 0) {
						error.value = 'This task could not be found.';
						return;
					}
					await syncActiveLocalWorkspace(workspace.id);
					store.commit('updateUserWorkspaceSetting', { workspaceId: workspace.id });
					store.commit('setCurrentTaskIdForModal', taskId);
					ready.value = true;
				} catch (e) {
					console.error('Error opening task window:', e);
					error.value = 'The task could not be opened. Close this window and try again.';
				}
			};
			void open();

			return { ready, error, closeWindow };
		},
	});
</script>
