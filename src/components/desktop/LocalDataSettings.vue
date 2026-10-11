<template>
	<div class="flex flex-col gap-6">
		<p v-if="loadError" role="alert" class="text-sm text-destructive">
			{{ loadError }}
		</p>
		<SettingsSection
			v-for="row in rows"
			:key="row.workspace.code"
			:title="row.workspace.name"
			description="No cloud copy. Backups stay on this computer."
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
				<SettingsRow label="Backups folder">
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
						:disabled="row.busy"
						:aria-busy="row.busy"
						@click="backup(row)"
					>
						{{ row.busy ? 'Backing up…' : 'Back up now' }}
					</Button>
				</SettingsRow>
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
	import { listLocalWorkspaces } from '@/local/runtime';
	import type { LocalWorkspace } from '@/local/types';
	import SettingsRow from '@/components/layouts/SettingsRow.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import { Button } from '@/components/ui/button';
	import { revealInFileManagerLabel } from '@/utils/desktop';
	import { lastBackupLabel } from '@/utils/localOnly';
	import { defineComponent, onMounted, ref } from 'vue';

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
		components: { SettingsRow, SettingsSection, Button },
		setup() {
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

			onMounted(load);

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

			return {
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
