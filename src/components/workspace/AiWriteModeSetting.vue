<template>
	<SettingsSection title="AI assistant">
		<SettingsRow
			v-slot="{ labelId }"
			label="Write mode"
			description="When enabled, the AI assistant can propose changes (create tasks, add comments). Nothing changes until you confirm."
		>
			<Select
				:model-value="mode"
				:disabled="!isOwner || loading || saving"
				@update:model-value="onChange"
			>
				<SelectTrigger
					class="w-full sm:w-56"
					:aria-labelledby="labelId"
					data-testid="ai-write-mode-select"
				>
					<SelectValue />
				</SelectTrigger>
				<SelectContent>
					<SelectItem value="read_only">Read only (default)</SelectItem>
					<SelectItem value="write_with_confirmation">
						Write with confirmation
					</SelectItem>
				</SelectContent>
			</Select>
		</SettingsRow>
		<p v-if="error" class="mt-2 text-sm text-destructive" role="alert">
			{{ error }}
		</p>
	</SettingsSection>
</template>

<script lang="ts">
	import {
		getWorkspaceSettings,
		setWorkspaceSetting,
	} from '@/actions/tmgr/workspaces';
	import SettingsRow from '@/components/layouts/SettingsRow.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import {
		Select,
		SelectContent,
		SelectItem,
		SelectTrigger,
		SelectValue,
	} from '@/components/ui/select';
	import {
		AI_WRITE_MODE_KEY,
		parseAiWriteMode,
		type AiWriteMode,
	} from '@/utils/aiWriteMode';
	import { readSettingValue } from '@/utils/pages/analyst';
	import { defineComponent, onMounted, ref } from 'vue';

	export default defineComponent({
		name: 'AiWriteModeSetting',
		components: {
			Select,
			SelectContent,
			SelectItem,
			SelectTrigger,
			SelectValue,
			SettingsRow,
			SettingsSection,
		},
		props: {
			workspaceId: { type: Number, required: true },
			isOwner: { type: Boolean, default: false },
		},
		setup(props) {
			const mode = ref<AiWriteMode>('read_only');
			const loading = ref(true);
			const saving = ref(false);
			const error = ref('');

			onMounted(async () => {
				try {
					const settings = await getWorkspaceSettings(props.workspaceId);
					mode.value = parseAiWriteMode(
						readSettingValue(settings, AI_WRITE_MODE_KEY),
					);
				} catch {
					error.value = 'Failed to load the setting';
				} finally {
					loading.value = false;
				}
			});

			const onChange = async (value: unknown) => {
				const next = parseAiWriteMode(value);
				const previous = mode.value;
				mode.value = next;
				saving.value = true;
				error.value = '';
				try {
					await setWorkspaceSetting(props.workspaceId, AI_WRITE_MODE_KEY, next);
				} catch {
					mode.value = previous;
					error.value = 'Failed to save the setting';
				} finally {
					saving.value = false;
				}
			};

			return { mode, loading, saving, error, onChange };
		},
	});
</script>
