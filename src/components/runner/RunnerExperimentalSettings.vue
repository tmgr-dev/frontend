<template>
	<div class="flex flex-col gap-6">
		<SettingsSection title="Agent runner">
			<div class="flex flex-col gap-1">
				<SettingsRow
					v-slot="{ labelId, descriptionId }"
					label="Agent runner"
					description="Run AI agents on local-workspace tasks through the TMGR runner on this computer. Experimental."
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
					<SettingsRow label="Runner status" :description="detail ?? undefined">
						<span class="text-sm text-ink" data-testid="runner-status">{{
							statusLine(runner.status.value)
						}}</span>
					</SettingsRow>
					<SettingsRow
						label="Permission prompts"
						description="Bypass mode can be enabled only from the terminal with `tmgr config allow-bypass`."
					>
						<span class="text-sm text-ink" data-testid="runner-permissions"
							>on</span
						>
					</SettingsRow>
					<SettingsRow
						v-if="bypassOffered"
						label="Bypass mode"
						description="Lets agents run without permission prompts."
					>
						<Button
							variant="outline"
							size="sm"
							data-testid="bypass-open"
							@click="bypassOpen = true"
						>
							Review and enable
						</Button>
					</SettingsRow>
				</template>
				<p v-if="error" class="pt-2 text-sm text-destructive" role="alert">
					{{ error }}
				</p>
			</div>
		</SettingsSection>

		<SettingsSection v-if="runner.enabled.value" title="Agent sessions">
			<AgentSessionsPanel />
		</SettingsSection>

		<BypassConsentDialog
			v-model:open="bypassOpen"
			@confirm="bypassOpen = false"
		/>
	</div>
</template>

<script lang="ts">
	import SettingsRow from '@/components/layouts/SettingsRow.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import AgentSessionsPanel from '@/components/runner/AgentSessionsPanel.vue';
	import BypassConsentDialog from '@/components/runner/BypassConsentDialog.vue';
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
