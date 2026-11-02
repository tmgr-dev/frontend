<template>
	<Dialog :open="open" @update:open="(value: boolean) => !value && close()">
		<DialogContent class="max-w-md">
			<DialogHeader>
				<DialogTitle>Your account is ready</DialogTitle>
				<DialogDescription>
					Your local workspaces stay on this device. Your new cloud workspace is
					next to them.
				</DialogDescription>
			</DialogHeader>

			<div class="grid gap-3 text-sm sm:grid-cols-2">
				<div>
					<h3 class="mb-1 font-semibold">On this device</h3>
					<ul class="flex flex-col gap-1" data-testid="local-upgrade-local">
						<li v-for="workspace in localWorkspaces" :key="workspace.code">
							{{ workspace.name }}
						</li>
					</ul>
				</div>
				<div>
					<h3 class="mb-1 font-semibold">In the cloud</h3>
					<p data-testid="local-upgrade-cloud">{{ cloudWorkspace?.name }}</p>
				</div>
			</div>

			<p v-if="keySentence" class="text-sm" data-testid="local-upgrade-key">
				{{ keySentence }}
			</p>
			<div
				v-if="state.phase === 'failed'"
				role="alert"
				class="flex items-start gap-2 text-sm text-amber-700 dark:text-amber-300"
				data-testid="local-upgrade-warning"
			>
				<TriangleAlert class="mt-0.5 size-4 shrink-0" />
				<span class="min-w-0 flex-1"
					>Your AI provider key could not be moved yet. You can keep working,
					and we will try again next time you start the app.</span
				>
				<Button size="sm" variant="outline" @click="retry">Retry</Button>
			</div>

			<DialogFooter>
				<Button
					variant="outline"
					data-testid="local-upgrade-open-cloud"
					@click="openCloud"
					>Open the cloud workspace</Button
				>
				<Button data-testid="local-upgrade-stay" @click="close"
					>Stay in the local workspace</Button
				>
			</DialogFooter>
		</DialogContent>
	</Dialog>
</template>

<script lang="ts">
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { listLocalWorkspaces } from '@/local/runtime';
	import type { LocalWorkspace } from '@/local/types';
	import {
		clearUpgradeDialog,
		retryLocalUpgrade,
		upgradeState,
	} from '@/utils/localUpgrade';
	import { syncActiveLocalWorkspace } from '@/utils/localWorkspaceSync';
	import { TriangleAlert } from 'lucide-vue-next';
	import { computed, defineComponent, ref, watch } from 'vue';
	import { useRouter } from 'vue-router';
	import { useStore } from 'vuex';

	export default defineComponent({
		name: 'LocalUpgradeHost',
		components: {
			Button,
			Dialog,
			DialogContent,
			DialogDescription,
			DialogFooter,
			DialogHeader,
			DialogTitle,
			TriangleAlert,
		},
		setup() {
			const store = useStore();
			const router = useRouter();
			const localWorkspaces = ref<LocalWorkspace[]>([]);
			const checked = ref(false);

			const cloudWorkspace = computed(() => {
				const list = ((store.state.workspaces ?? []) as any[]).filter(
					(workspace) => !workspace.is_local && Number(workspace.id) > 0,
				);
				const preferred = Number((store.state as any).defaultWorkspaceId);
				return list.find((w) => Number(w.id) === preferred) ?? list[0] ?? null;
			});

			const ready = computed(
				() =>
					upgradeState.dialog &&
					store.getters.accountMode === 'cloud' &&
					Number(store.state.user?.id) > 0 &&
					upgradeState.phase !== 'running',
			);

			watch(
				ready,
				async (value) => {
					if (!value || checked.value) return;
					checked.value = true;
					localWorkspaces.value = await listLocalWorkspaces();
					if (!localWorkspaces.value.length) clearUpgradeDialog();
				},
				{ immediate: true },
			);

			const open = computed(
				() =>
					ready.value &&
					localWorkspaces.value.length > 0 &&
					cloudWorkspace.value !== null,
			);

			const keySentence = computed(() => {
				if (upgradeState.phase !== 'done') return '';
				if (upgradeState.key === 'moved' || upgradeState.key === 'already')
					return 'Your AI provider key moved to this account. Signing out removes it from this device.';
				if (upgradeState.key === 'conflict')
					return 'This account already has an AI provider key, so it was kept. Your local key is still saved for local mode.';
				return '';
			});

			const close = () => clearUpgradeDialog();

			const openCloud = async () => {
				const workspace = cloudWorkspace.value;
				clearUpgradeDialog();
				if (!workspace) return;
				await syncActiveLocalWorkspace(workspace.id);
				store.commit('updateUserWorkspaceSetting', {
					workspaceId: workspace.id,
				});
				await store.dispatch(
					'featureToggles/loadWorkspaceToggles',
					workspace.id,
				);
				const landing =
					store.getters['featureToggles/getUserFeatureValue'](
						'default_landing_page',
					) || 'list';
				await router.push(`/${workspace.code}/${landing}`);
				store.commit('rerenderApp');
			};

			return {
				state: upgradeState,
				open,
				localWorkspaces,
				cloudWorkspace,
				keySentence,
				close,
				openCloud,
				retry: () => retryLocalUpgrade(),
			};
		},
	});
</script>
