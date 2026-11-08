<template>
	<SettingsSection :title="$t('settings.language.title')">
		<SettingsRow
			:label="$t('settings.language.label')"
			:description="$t('settings.language.description')"
		>
			<select
				:value="current"
				:disabled="saving"
				class="h-10 w-full rounded-md border border-input bg-background px-3 text-sm disabled:cursor-not-allowed disabled:opacity-50 sm:w-72"
				data-testid="language-select"
				@change="onChange"
			>
				<option
					v-for="option in options"
					:key="option.value"
					:value="option.value"
				>
					{{ option.label }}
				</option>
			</select>
		</SettingsRow>
		<p v-if="error" class="mt-2 text-sm text-destructive" role="alert">
			{{ error }}
		</p>
	</SettingsSection>
</template>

<script lang="ts">
	import { saveUserLanguage } from '@/actions/tmgr/user';
	import SettingsRow from '@/components/layouts/SettingsRow.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import { i18n, LOCALE_NAMES, setLocale, SUPPORTED_LOCALES, t } from '@/i18n';
	import { computed, defineComponent, ref } from 'vue';

	export default defineComponent({
		name: 'LanguageSetting',
		components: { SettingsRow, SettingsSection },
		setup() {
			const saving = ref(false);
			const error = ref('');
			const current = computed(() => i18n.global.locale.value);
			const options = SUPPORTED_LOCALES.map((value) => ({
				value,
				label: LOCALE_NAMES[value],
			}));

			const onChange = async (event: Event) => {
				const value = (event.target as HTMLSelectElement).value;
				if (value === current.value) return;
				saving.value = true;
				error.value = '';
				try {
					const locale = await setLocale(value);
					await saveUserLanguage(locale);
				} catch {
					error.value = t('settings.language.saveFailed');
				} finally {
					saving.value = false;
				}
			};

			return { current, options, saving, error, onChange };
		},
	});
</script>
