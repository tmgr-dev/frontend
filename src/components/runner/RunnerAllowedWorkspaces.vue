<template>
	<SettingsSection
		title="Workspaces allowed for cloud runs"
		description="Workspaces whose approved tasks your agents may run on this computer. Allowing happens only in a terminal; this list is read-only."
	>
		<p v-if="error" class="text-sm text-destructive" role="alert">
			{{ error }}
		</p>
		<p
			v-else-if="loaded && !entries.length"
			class="text-sm text-ink-subtle"
			data-testid="allowed-empty"
		>
			No workspace is allowed yet. In a terminal run
			<CommandChip command="tmgr runner allow <code>" />
		</p>
		<div v-else class="divide-y divide-border">
			<SettingsRow
				v-for="entry in entries"
				:key="entry.workspaceId"
				:label="entry.workspaceCode"
				:data-testid="`allowed-${entry.workspaceCode}`"
			>
				<template #description>
					{{ expiryText(entry) }} · {{ entry.keys }}
					{{ entry.keys === 1 ? 'key' : 'keys' }} · {{ entry.repos }}
					{{ entry.repos === 1 ? 'repository' : 'repositories' }}
					<span
						v-for="item in entry.repositories"
						:key="item.categoryId"
						class="mt-0.5 block text-xs"
						:data-testid="`allowed-repos-${entry.workspaceCode}-${item.categoryId}`"
					>
						Category {{ item.categoryId }}:
						<span class="font-mono">{{ item.names.join(', ') }}</span>
					</span>
				</template>
				<CommandChip :command="allowCommand(entry.workspaceCode)" />
			</SettingsRow>
		</div>
	</SettingsSection>
</template>

<script lang="ts">
	import CommandChip from '@/components/general/CommandChip.vue';
	import SettingsRow from '@/components/layouts/SettingsRow.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import { allowCommand } from '@/runner/allowReminder';
	import { runnerAllowedList } from '@/runner/api';
	import type { AllowedEntry } from '@/utils/agentApprovalPolicy';
	import { defineComponent, onMounted, ref } from 'vue';

	export default defineComponent({
		name: 'RunnerAllowedWorkspaces',
		components: { CommandChip, SettingsRow, SettingsSection },
		setup() {
			const entries = ref<AllowedEntry[]>([]);
			const error = ref('');
			const loaded = ref(false);

			onMounted(async () => {
				try {
					const list = await runnerAllowedList();
					entries.value = list.entries;
					error.value = list.error ?? '';
				} catch (failure) {
					error.value = String((failure as any)?.message ?? failure);
				} finally {
					loaded.value = true;
				}
			});

			const expiryText = (entry: AllowedEntry): string => {
				if (!entry.expiresAt) return 'No expiry';
				const when = new Date(entry.expiresAt).toLocaleDateString();
				return entry.expired ? `Expired ${when}` : `Expires ${when}`;
			};

			return { entries, error, loaded, expiryText, allowCommand };
		},
	});
</script>
