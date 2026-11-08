<template>
	<SettingsSection
		:title="$t('desktop.runner.allowed.title')"
		:description="$t('desktop.runner.allowed.description')"
	>
		<p v-if="error" class="text-sm text-destructive" role="alert">
			{{ error }}
		</p>
		<p
			v-else-if="loaded && !entries.length"
			class="text-sm text-ink-subtle"
			data-testid="allowed-empty"
		>
			<i18n-t keypath="desktop.runner.allowed.empty" tag="span">
				<template #command>
					<CommandChip command="tmgr runner allow <code>" />
				</template>
			</i18n-t>
		</p>
		<div v-else class="divide-y divide-border">
			<SettingsRow
				v-for="entry in entries"
				:key="entry.workspaceId"
				:label="entry.workspaceCode"
				:data-testid="`allowed-${entry.workspaceCode}`"
			>
				<template #description>
					{{ expiryText(entry) }} ·
					{{ $t('desktop.runner.allowed.keys', entry.keys) }} ·
					{{ $t('desktop.runner.allowed.repositories', entry.repos) }}
					<span
						v-for="item in entry.repositories"
						:key="item.categoryId"
						class="mt-0.5 block text-xs"
						:data-testid="`allowed-repos-${entry.workspaceCode}-${item.categoryId}`"
					>
						{{ $t('desktop.runner.allowed.category', { id: item.categoryId }) }}
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
	import { currentLocale, t } from '@/i18n';
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
				if (!entry.expiresAt) return t('desktop.runner.allowed.noExpiry');
				const when = new Date(entry.expiresAt).toLocaleDateString(
					currentLocale(),
				);
				return entry.expired
					? t('desktop.runner.allowed.expired', { when })
					: t('desktop.runner.allowed.expires', { when });
			};

			return { entries, error, loaded, expiryText, allowCommand };
		},
	});
</script>
