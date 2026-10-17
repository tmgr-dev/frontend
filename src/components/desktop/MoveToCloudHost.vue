<template>
	<MoveToCloudDialog
		:open="dialogOpen"
		:code="moveUi.dialogCode ?? ''"
		:name="nameOf(moveUi.dialogCode)"
		@update:open="(open: boolean) => !open && (moveUi.dialogCode = null)"
	/>
	<MoveToCloudProgress
		:open="progressOpen"
		:code="moveUi.progressCode ?? ''"
		:name="nameOf(moveUi.progressCode)"
		@update:open="(open: boolean) => !open && (moveUi.progressCode = null)"
	/>
</template>

<script lang="ts">
	import MoveToCloudDialog from '@/components/desktop/MoveToCloudDialog.vue';
	import MoveToCloudProgress from '@/components/desktop/MoveToCloudProgress.vue';
	import { waitForUser } from '@/local/moveToCloud/account';
	import { browserStorage, loadPersisted } from '@/local/moveToCloud/model';
	import { getMover, userSnapshot } from '@/local/moveToCloud/runtime';
	import { moveUi } from '@/local/moveToCloud/ui';
	import { listLocalWorkspaces } from '@/local/runtime';
	import type { LocalWorkspace } from '@/local/types';
	import { desktopOfferEnv, offerMoveToCloud } from '@/tours/desktopOffers';
	import { toursEngine } from '@/tours/useTours';
	import { computed, defineComponent, onMounted, ref, watch } from 'vue';
	import { useStore } from 'vuex';

	export default defineComponent({
		name: 'MoveToCloudHost',
		components: { MoveToCloudDialog, MoveToCloudProgress },
		setup() {
			const store = useStore();
			const workspaces = ref<LocalWorkspace[]>([]);

			const retryOffer = () => {
				if (store.getters.isLocalOnly) return;
				void offerMoveToCloud(
					desktopOfferEnv(async () => workspaces.value.length > 0),
				);
			};
			watch(
				() => toursEngine().store.loaded.value,
				(loaded) => {
					if (loaded) retryOffer();
				},
			);

			onMounted(async () => {
				if (store.getters.isLocalOnly) return;
				workspaces.value = await listLocalWorkspaces();
				retryOffer();
				const pending = workspaces.value.filter(
					(workspace) => loadPersisted(browserStorage, workspace.code).job,
				);
				if (!pending.length) return;
				const account = await waitForUser(
					userSnapshot,
					(ms) => new Promise((resolve) => setTimeout(resolve, ms)),
				);
				if (!account.known) return;
				for (const workspace of pending) {
					void getMover().resume(workspace.code);
				}
			});

			watch(
				() => [moveUi.dialogCode, moveUi.progressCode],
				async () => {
					workspaces.value = await listLocalWorkspaces();
				},
			);

			const nameOf = (code: string | null) =>
				workspaces.value.find((w) => w.code === code)?.name ?? '';

			return {
				moveUi,
				nameOf,
				dialogOpen: computed(() => moveUi.dialogCode !== null),
				progressOpen: computed(() => moveUi.progressCode !== null),
			};
		},
	});
</script>
