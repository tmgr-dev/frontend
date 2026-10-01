<template>
	<SettingsSection
		title="Страницы"
		description="Персона-аналитик ведёт управляемую секцию «Инсайты» на страницах людей."
	>
		<SettingsRow
			v-slot="{ labelId }"
			label="Персона-аналитик для страниц"
			description="Не меняет уже созданные страницы."
		>
			<Select
				:model-value="selected ?? NONE"
				:disabled="!isOwner || loading || saving"
				@update:model-value="onChange"
			>
				<SelectTrigger
					class="w-full sm:w-56"
					:aria-labelledby="labelId"
					data-testid="analyst-select"
				>
					<SelectValue />
				</SelectTrigger>
				<SelectContent>
					<SelectItem :value="NONE">Не выбрана</SelectItem>
					<SelectItem
						v-for="option in options"
						:key="option.value"
						:value="option.value"
					>
						{{ option.label }}
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
	import { listWorkspacePersonas } from '@/actions/tmgr/personas';
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
		ANALYST_SETTING_KEY,
		analystOptions,
		readSettingValue,
		type AnalystOption,
	} from '@/utils/pages/analyst';
	import { defineComponent, onMounted, ref } from 'vue';

	const NONE = '__none__';

	export default defineComponent({
		name: 'PagesAnalystSetting',
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
			const selected = ref<string | null>(null);
			const options = ref<AnalystOption[]>([]);
			const loading = ref(true);
			const saving = ref(false);
			const error = ref('');

			onMounted(async () => {
				const [settings, grants] = await Promise.allSettled([
					getWorkspaceSettings(props.workspaceId),
					listWorkspacePersonas(props.workspaceId),
				]);
				if (settings.status === 'fulfilled') {
					selected.value =
						readSettingValue(settings.value, ANALYST_SETTING_KEY) ?? null;
				}
				if (grants.status === 'fulfilled') {
					options.value = analystOptions(grants.value, selected.value);
				}
				loading.value = false;
			});

			const onChange = async (value: unknown) => {
				const next = value === NONE ? null : String(value);
				const previous = selected.value;
				selected.value = next;
				saving.value = true;
				error.value = '';
				try {
					await setWorkspaceSetting(
						props.workspaceId,
						ANALYST_SETTING_KEY,
						next,
					);
				} catch {
					selected.value = previous;
					error.value = 'Не удалось сохранить настройку';
				} finally {
					saving.value = false;
				}
			};

			return { selected, options, loading, saving, error, onChange, NONE };
		},
	});
</script>
