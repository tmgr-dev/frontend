<template>
	<div class="flex flex-col gap-6">
		<SettingsSection :title="$t('desktop.runner.settings.title')">
			<div class="flex flex-col gap-1">
				<SettingsRow
					v-slot="{ labelId, descriptionId }"
					:label="$t('desktop.runner.settings.title')"
					:description="$t('desktop.runner.settings.description')"
				>
					<Switch
						:checked="runner.enabled.value"
						:disabled="busy"
						:aria-labelledby="labelId"
						:aria-describedby="descriptionId"
						data-testid="runner-switch"
						@update:checked="toggle"
					/>
				</SettingsRow>
				<template v-if="runner.enabled.value">
					<SettingsRow :label="$t('desktop.runner.settings.statusLabel')">
						<template v-if="detail" #description>
							<CommandText :text="detail" />
						</template>
						<span class="text-sm text-ink" data-testid="runner-status">{{
							statusLine(runner.status.value)
						}}</span>
					</SettingsRow>
					<SettingsRow :label="$t('desktop.runner.settings.permissionPrompts')">
						<template #description>
							<i18n-t
								keypath="desktop.runner.settings.bypassTerminalOnly"
								tag="span"
							>
								<template #command>
									<CommandChip command="tmgr config allow-bypass" />
								</template>
							</i18n-t>
						</template>
						<span class="text-sm text-ink" data-testid="runner-permissions">{{
							$t('common.states.on')
						}}</span>
					</SettingsRow>
					<SettingsRow
						v-if="bypassOffered"
						:label="$t('desktop.runner.settings.bypassMode')"
						:description="$t('desktop.runner.settings.bypassDescription')"
					>
						<Button
							variant="outline"
							size="sm"
							data-testid="bypass-open"
							@click="bypassOpen = true"
						>
							{{ $t('desktop.runner.settings.reviewAndEnable') }}
						</Button>
					</SettingsRow>
				</template>
				<p v-if="error" class="pt-2 text-sm text-destructive" role="alert">
					{{ error }}
				</p>
			</div>
		</SettingsSection>

		<SettingsSection
			v-if="runner.enabled.value"
			:title="$t('desktop.runner.settings.sessionsTitle')"
		>
			<AgentSessionsPanel />
		</SettingsSection>

		<RunnerAllowedWorkspaces v-if="runner.enabled.value" />

		<BypassConsentDialog
			v-model:open="bypassOpen"
			@confirm="bypassOpen = false"
		/>
	</div>
</template>

<script lang="ts">
	import CommandChip from '@/components/general/CommandChip.vue';
	import CommandText from '@/components/general/CommandText.vue';
	import SettingsRow from '@/components/layouts/SettingsRow.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import AgentSessionsPanel from '@/components/runner/AgentSessionsPanel.vue';
	import BypassConsentDialog from '@/components/runner/BypassConsentDialog.vue';
	import RunnerAllowedWorkspaces from '@/components/runner/RunnerAllowedWorkspaces.vue';
	import { Button } from '@/components/ui/button';
	import { Switch } from '@/components/ui/switch';
	import { bypassAvailable } from '@/runner/bypass';
	import { statusDetail, statusLine } from '@/runner/statusLine';
	import {
		refreshRunnerStatus,
		setRunnerEnabled,
		useRunner,
	} from '@/runner/store';
	import { computed, defineComponent, onMounted, ref } from 'vue';

	export default defineComponent({
		name: 'RunnerExperimentalSettings',
		components: {
			AgentSessionsPanel,
			BypassConsentDialog,
			Button,
			CommandChip,
			CommandText,
			RunnerAllowedWorkspaces,
			SettingsRow,
			SettingsSection,
			Switch,
		},
		setup() {
			const runner = useRunner();
			const busy = ref(false);
			const error = ref('');
			const bypassOpen = ref(false);
			const bypassOffered = computed(() =>
				bypassAvailable(runner.status.value),
			);

			const detail = computed(() => statusDetail(runner.status.value));

			onMounted(() => void refreshRunnerStatus());

			const toggle = async (value: boolean) => {
				busy.value = true;
				error.value = '';
				try {
					await setRunnerEnabled(value);
				} catch (failure) {
					error.value = String((failure as any)?.message ?? failure);
				} finally {
					busy.value = false;
				}
			};

			return {
				runner,
				busy,
				error,
				bypassOpen,
				bypassOffered,
				detail,
				toggle,
				statusLine,
			};
		},
	});
</script>
