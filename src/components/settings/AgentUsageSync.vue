<template>
	<div class="flex flex-col gap-6" data-testid="agent-usage-sync">
		<SettingsSection
			title="Agent work"
			description="Show what AI agents cost on your tasks, on every device."
		>
			<SettingsRow
				v-slot="{ labelId, descriptionId }"
				label="Agent usage sync"
				description="Uploads per-task totals so web and mobile can show them: tokens, AI cost (API-equivalent), agent, model and tool names, and counts. Never prompts, replies, file paths or command arguments. Uploads start only from the tmgr CLI, after you turn them on there: by hand with `tmgr usage sync`, or after each agent run if you also confirm `tmgr usage sync auto on`. Turning it off deletes everything already uploaded."
			>
				<Switch
					data-testid="agent-usage-sync-switch"
					:checked="on"
					:disabled="!available || busy"
					:aria-labelledby="labelId"
					:aria-describedby="descriptionId"
					@update:checked="onToggle"
				/>
			</SettingsRow>
			<p v-if="!available" class="mt-2 text-sm text-ink-subtle">
				Agent usage sync is not available on this server yet.
			</p>
			<p v-if="error" class="mt-2 text-sm text-destructive" role="alert">
				{{ error }}
			</p>
		</SettingsSection>

		<SettingsSection
			v-if="available"
			title="Your uploads"
			description="What this account has uploaded, newest first."
		>
			<template #actions>
				<Button
					variant="outline"
					size="sm"
					:disabled="busy || loading"
					data-testid="agent-usage-delete-all"
					@click="confirmDelete = true"
				>
					Delete all
				</Button>
			</template>
			<p v-if="loading" class="text-sm text-ink-subtle" aria-live="polite">
				Loading…
			</p>
			<p v-else-if="!rows.length" class="text-sm text-ink-subtle">
				Nothing uploaded.
			</p>
			<ul v-else class="divide-y divide-line text-sm">
				<li
					v-for="row in rows"
					:key="row.key"
					class="flex flex-wrap items-baseline gap-x-3 py-2"
				>
					<span class="font-medium text-ink">{{ row.task }}</span>
					<span v-if="row.cost" class="font-mono text-ink-subtle">{{
						row.cost
					}}</span>
					<span class="font-mono text-ink-subtle">{{ row.tokens }}</span>
					<span class="ml-auto text-xs text-ink-subtle">{{
						ago(row.updated)
					}}</span>
				</li>
			</ul>
		</SettingsSection>

		<AlertDialog v-model:open="confirmOff">
			<AlertDialogContent>
				<AlertDialogHeader>
					<AlertDialogTitle>Turn off Agent usage sync?</AlertDialogTitle>
					<AlertDialogDescription>
						Everything this account has uploaded is deleted from TMGR. You can
						turn it on again later.
					</AlertDialogDescription>
				</AlertDialogHeader>
				<AlertDialogFooter>
					<AlertDialogCancel>Keep it on</AlertDialogCancel>
					<AlertDialogAction
						class="bg-destructive text-destructive-foreground hover:bg-destructive/90"
						@click="turnOff"
						>Turn off and delete</AlertDialogAction
					>
				</AlertDialogFooter>
			</AlertDialogContent>
		</AlertDialog>

		<AlertDialog v-model:open="confirmDelete">
			<AlertDialogContent>
				<AlertDialogHeader>
					<AlertDialogTitle>Delete all uploads?</AlertDialogTitle>
					<AlertDialogDescription>
						Everything this account has uploaded is deleted from TMGR. Agent
						usage sync stays as it is.
					</AlertDialogDescription>
				</AlertDialogHeader>
				<AlertDialogFooter>
					<AlertDialogCancel>Cancel</AlertDialogCancel>
					<AlertDialogAction
						class="bg-destructive text-destructive-foreground hover:bg-destructive/90"
						@click="deleteAll"
						>Delete all</AlertDialogAction
					>
				</AlertDialogFooter>
			</AlertDialogContent>
		</AlertDialog>
	</div>
</template>

<script lang="ts">
	import {
		deleteAllAgentUsage,
		isAgentUsageSyncAvailable,
		isAgentUsageSyncOn,
		listAgentUsage,
		setAgentUsageSync,
		uploadRows,
		type UploadRow,
	} from '@/actions/tmgr/agentUsage';
	import SettingsRow from '@/components/layouts/SettingsRow.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import {
		AlertDialog,
		AlertDialogAction,
		AlertDialogCancel,
		AlertDialogContent,
		AlertDialogDescription,
		AlertDialogFooter,
		AlertDialogHeader,
		AlertDialogTitle,
	} from '@/components/ui/alert-dialog';
	import { Button } from '@/components/ui/button';
	import { Switch } from '@/components/ui/switch';
	import { formatDistanceToNow } from 'date-fns';
	import { computed, defineComponent, onMounted, ref } from 'vue';
	import { useStore } from 'vuex';

	const message = (e: unknown) =>
		(e as any)?.response?.data?.message ||
		(e as Error)?.message ||
		'Something went wrong.';

	export default defineComponent({
		name: 'AgentUsageSync',
		components: {
			SettingsRow,
			SettingsSection,
			Switch,
			Button,
			AlertDialog,
			AlertDialogAction,
			AlertDialogCancel,
			AlertDialogContent,
			AlertDialogDescription,
			AlertDialogFooter,
			AlertDialogHeader,
			AlertDialogTitle,
		},
		setup() {
			const store = useStore();
			const rows = ref<UploadRow[]>([]);
			const loading = ref(false);
			const busy = ref(false);
			const error = ref('');
			const confirmOff = ref(false);
			const confirmDelete = ref(false);

			const available = computed(() => {
				void store.state.user;
				return isAgentUsageSyncAvailable();
			});
			const on = computed(() => {
				void store.state.user;
				return isAgentUsageSyncOn();
			});

			let latest = 0;

			const load = async () => {
				if (!isAgentUsageSyncAvailable()) return;
				const current = ++latest;
				loading.value = true;
				try {
					const fetched = uploadRows(await listAgentUsage());
					if (current === latest) rows.value = fetched;
				} catch (e) {
					error.value = message(e);
				} finally {
					if (current === latest) loading.value = false;
				}
			};

			const apply = async (value: boolean) => {
				busy.value = true;
				error.value = '';
				try {
					await setAgentUsageSync(value);
				} catch (e) {
					error.value = message(e);
				} finally {
					busy.value = false;
				}
				await load();
			};

			onMounted(load);

			return {
				rows,
				loading,
				busy,
				error,
				confirmOff,
				confirmDelete,
				available,
				on,
				onToggle: (value: boolean) => {
					if (value) apply(true);
					else confirmOff.value = true;
				},
				turnOff: () => apply(false),
				deleteAll: async () => {
					busy.value = true;
					error.value = '';
					try {
						await deleteAllAgentUsage();
					} catch (e) {
						error.value = message(e);
					} finally {
						busy.value = false;
					}
					await load();
				},
				ago: (iso: string) => {
					const date = new Date(iso);
					return Number.isNaN(date.getTime())
						? ''
						: formatDistanceToNow(date, { addSuffix: true });
				},
			};
		},
	});
</script>
