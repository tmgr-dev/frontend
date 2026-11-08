<template>
	<SettingsSection :title="$t('workspace.agentRunsSetting.title')">
		<SettingsRow
			v-slot="{ labelId, descriptionId }"
			:label="$t('workspace.agentRunsSetting.label')"
			:description="$t('workspace.agentRunsSetting.description')"
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
	import { t } from '@/i18n';
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
					error.value = t('workspace.agentRunsSetting.loadFailed');
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
							? t('workspace.agentRunsSetting.uiSessionRequired')
							: t('workspace.agentRunsSetting.saveFailed');
				} finally {
					saving.value = false;
				}
			};

			return { enabled, loading, saving, error, toggle };
		},
	});
</script>
