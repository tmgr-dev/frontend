<template>
	<SettingsSection title="Agent runs">
		<SettingsRow
			v-slot="{ labelId, descriptionId }"
			label="Let my agents run my tasks"
			:description="DESCRIPTION"
		>
			<Switch
				:checked="enabled"
				:disabled="loading || saving"
				:aria-labelledby="labelId"
				:aria-describedby="descriptionId"
				data-testid="agent-runs-switch"
				@update:checked="toggle"
			/>
		</SettingsRow>
		<p v-if="error" class="mt-2 text-sm text-destructive" role="alert">
			{{ error }}
		</p>
	</SettingsSection>
</template>

<script lang="ts">
	import {
		agentRunErrorCode,
		getAgentRunsSetting,
		setAgentRunsSetting,
	} from '@/actions/tmgr/agentRuns';
	import SettingsRow from '@/components/layouts/SettingsRow.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import { Switch } from '@/components/ui/switch';
	import { rememberAgentRunsSetting } from '@/utils/agentApprovalClient';
	import { defineComponent, onMounted, ref } from 'vue';

	const DESCRIPTION =
		'Only tasks you wrote, or tasks you confirmed for agent runs, can be started by your agents on your own machine. An approval is cancelled automatically when someone else or an agent changes the task. To start runs you also allow this workspace on your machine with `tmgr runner allow <code>`.';

	export default defineComponent({
		name: 'AgentRunsSetting',
		components: { SettingsRow, SettingsSection, Switch },
		props: { workspaceId: { type: Number, required: true } },
		setup(props) {
			const enabled = ref(false);
			const loading = ref(true);
			const saving = ref(false);
			const error = ref('');

			onMounted(async () => {
				try {
					enabled.value = await getAgentRunsSetting(props.workspaceId);
				} catch {
					error.value = 'Could not load this setting.';
				} finally {
					loading.value = false;
				}
			});

			const toggle = async (value: boolean) => {
				saving.value = true;
				error.value = '';
				try {
					enabled.value = await setAgentRunsSetting(props.workspaceId, value);
					rememberAgentRunsSetting(props.workspaceId, enabled.value);
				} catch (failure) {
					error.value =
						agentRunErrorCode(failure) === 'ui_session_required'
							? 'Change this from the web or desktop app.'
							: 'Could not save this setting.';
				} finally {
					saving.value = false;
				}
			};

			return { enabled, loading, saving, error, toggle, DESCRIPTION };
		},
	});
</script>
