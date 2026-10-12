<template>
	<div class="flex flex-col gap-6">
		<p v-if="loadError" role="alert" class="text-sm text-destructive">
			{{ loadError }}
		</p>
		<SettingsSection
			v-for="row in rows"
			:key="row.workspace.code"
			:title="row.workspace.name"
			:description="sectionDescription(row)"
		>
			<div class="divide-y divide-border">
				<SettingsRow label="Data folder">
					<template #description>
						<span class="break-all font-mono text-xs">{{
							row.workspace.path
						}}</span>
					</template>
					<Button variant="outline" size="sm" @click="openData(row)">
						{{ revealLabel }}
					</Button>
				</SettingsRow>
				<SettingsRow label="Backups folder" data-tour="settings.data-backups">
					<template #description>
						<span class="break-all font-mono text-xs">{{
							row.status?.backups_dir ?? ''
						}}</span>
					</template>
					<Button
						variant="outline"
						size="sm"
						:disabled="!row.status"
						@click="openBackups(row)"
					>
						Open folder
					</Button>
				</SettingsRow>
				<SettingsRow :label="lastBackup(row)">
					<template v-if="row.status?.last_error" #description>
						<span class="text-destructive">
							Last backup failed: {{ row.status.last_error }}
						</span>
					</template>
					<Button
						size="sm"
						data-tour="settings.backup-now"
						:disabled="row.busy"
						:aria-busy="row.busy"
						@click="backup(row)"
					>
						{{ row.busy ? 'Backing up…' : 'Back up now' }}
					</Button>
				</SettingsRow>
				<SettingsRow label="Move to cloud" data-tour="settings.data-move">
					<template #description>
						<template v-if="!signedIn">
							Sign in to move this workspace to your account.
						</template>
						<template v-else-if="moveState(row).job">
							A move is in progress.
						</template>
						<template v-else>
							Copies this workspace into a new workspace in your TMGR account. The
							local copy stays.
						</template>
					</template>
					<Button
						v-if="signedIn && moveState(row).job"
						variant="outline"
						size="sm"
						@click="openProgress(row.workspace.code)"
					>
						View progress
					</Button>
					<Button
						v-else
						variant="outline"
						size="sm"
						:disabled="!signedIn"
						@click="openDialog(row.workspace.code)"
					>
						Move to cloud…
					</Button>
				</SettingsRow>
			</div>
			<div
				v-if="signedIn && moveState(row).published"
				class="mt-4 border-t border-border pt-4"
			>
				<h3 class="mb-2 text-sm font-semibold">Remove local copy</h3>
				<RemoveLocalCopy
					:key="moveState(row).published!.jobId + moveState(row).published!.at"
					:workspace="row.workspace"
					@removed="load"
				/>
			</div>
			<p v-if="row.error" role="alert" class="mt-3 text-sm text-destructive">
				{{ row.error }}
			</p>
		</SettingsSection>
		<p v-if="loaded && !rows.length" class="text-sm text-ink-subtle">
			There are no local workspaces on this computer yet.
		</p>
	</div>
</template>

<script lang="ts">
	import {
		backupNow,
		backupStatus,
		revealBackups,
		revealWorkspaceFolder,
		type BackupStatus,
	} from '@/local/backups';
	import { formatMoveDate } from '@/local/moveToCloud/banner';
	import { browserStorage, loadPersisted } from '@/local/moveToCloud/model';
	import { getMover } from '@/local/moveToCloud/runtime';
	import { openMoveDialog, openMoveProgress } from '@/local/moveToCloud/ui';
	import { listLocalWorkspaces } from '@/local/runtime';
	import type { LocalWorkspace } from '@/local/types';
	import RemoveLocalCopy from '@/components/desktop/RemoveLocalCopy.vue';
	import SettingsRow from '@/components/layouts/SettingsRow.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import { Button } from '@/components/ui/button';
	import { revealInFileManagerLabel } from '@/utils/desktop';
	import { lastBackupLabel } from '@/utils/localOnly';
	import { computed, defineComponent, onBeforeUnmount, onMounted, ref } from 'vue';
	import { useStore } from 'vuex';

	interface Row {
		workspace: LocalWorkspace;
		status: BackupStatus | null;
		busy: boolean;
		error: string;
	}

	const message = (e: unknown) =>
		(e as any)?.message || String(e) || 'Something went wrong.';

	export default defineComponent({
		name: 'LocalDataSettings',
		components: { RemoveLocalCopy, SettingsRow, SettingsSection, Button },
		setup() {
			const store = useStore();
			const signedIn = computed(() => store.getters.isLocalOnly === false);
			const rows = ref<Row[]>([]);
			const loaded = ref(false);
			const loadError = ref('');

			const load = async () => {
				try {
					const [workspaces, statuses] = await Promise.all([
						listLocalWorkspaces(true),
						backupStatus(),
					]);
					rows.value = workspaces.map((workspace) => ({
						workspace,
						status: statuses.find((s) => s.code === workspace.code) ?? null,
						busy: false,
						error: '',
					}));
				} catch (e) {
					loadError.value = message(e);
				} finally {
					loaded.value = true;
				}
			};

			const moveTick = ref(0);
			let unsubscribe: (() => void) | null = null;
			onMounted(() => {
				unsubscribe = getMover().subscribe(() => moveTick.value++);
				void load();
			});
			onBeforeUnmount(() => unsubscribe?.());

			const run = async (row: Row, action: () => Promise<unknown>) => {
				row.error = '';
				try {
					await action();
				} catch (e) {
					row.error = message(e);
				}
			};

			const backup = async (row: Row) => {
				if (row.busy) return;
				row.busy = true;
				await run(row, backupNow.bind(null, row.workspace.code));
				try {
					const statuses = await backupStatus();
					row.status =
						statuses.find((s) => s.code === row.workspace.code) ?? row.status;
				} catch (e) {
					row.error ||= message(e);
				}
				row.busy = false;
			};

			const moveState = (row: Row) => {
				void moveTick.value;
				return loadPersisted(browserStorage, row.workspace.code);
			};
			const sectionDescription = (row: Row) => {
				const published = moveState(row).published;
				return published
					? `Copied to your cloud workspace ${published.name} on ${formatMoveDate(published.at)}. Backups stay on this computer.`
					: 'No cloud copy. Backups stay on this computer.';
			};

			return {
				signedIn,
				load,
				moveState,
				sectionDescription,
				openDialog: openMoveDialog,
				openProgress: openMoveProgress,
				rows,
				loaded,
				loadError,
				revealLabel: revealInFileManagerLabel(),
				lastBackup: (row: Row) => lastBackupLabel(row.status?.last_backup_at),
				openData: (row: Row) =>
					run(row, () => revealWorkspaceFolder(row.workspace.code)),
				openBackups: (row: Row) =>
					run(row, () => revealBackups(row.workspace.code)),
				backup,
			};
		},
	});
</script>
