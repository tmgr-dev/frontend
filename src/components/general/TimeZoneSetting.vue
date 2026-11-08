<template>
	<SettingsSection :title="$t('settings.timezone.title')">
		<SettingsRow
			:label="$t('settings.timezone.title')"
			:description="$t('settings.timezone.description')"
		>
			<div class="flex w-full flex-col gap-2 sm:w-72">
				<Combobox
					:model-value="current"
					:entities="options"
					label-key="label"
					value-key="value"
					:selected-placeholder="$t('settings.timezone.placeholder')"
					:input-placeholder="$t('settings.timezone.search')"
					:not-found-message="$t('settings.timezone.notFound')"
					:class="{ 'pointer-events-none opacity-60': saving }"
					data-testid="timezone-select"
					@update:model-value="onSelect"
				/>
				<p
					v-if="detected"
					class="flex flex-wrap items-center gap-2 text-sm text-ink-subtle"
				>
					<span>{{
						$t('settings.timezone.detected', { zone: detected })
					}}</span>
					<Button
						v-if="detected !== current"
						variant="outline"
						size="sm"
						:disabled="saving"
						@click="save(detected)"
					>
						{{ $t('settings.timezone.useDetected') }}
					</Button>
				</p>
			</div>
		</SettingsRow>
		<p v-if="error" class="mt-2 text-sm text-destructive" role="alert">
			{{ error }}
		</p>
	</SettingsSection>
</template>

<script lang="ts">
	import { saveUserTimeZone } from '@/actions/tmgr/user';
	import Combobox from '@/components/Combobox.vue';
	import SettingsRow from '@/components/layouts/SettingsRow.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import { Button } from '@/components/ui/button';
	import { t } from '@/i18n';
	import {
		browserTimeZone,
		timeZoneOptions,
		type TimeZoneOption,
	} from '@/utils/timezone';
	import { computed, defineComponent, ref } from 'vue';
	import { useStore } from 'vuex';

	export default defineComponent({
		name: 'TimeZoneSetting',
		components: { Button, Combobox, SettingsRow, SettingsSection },
		setup() {
			const store = useStore();
			const saving = ref(false);
			const error = ref('');
			const current = computed<string>(() => store.getters.userTimeZone);
			const detected = browserTimeZone();
			const options = computed<(TimeZoneOption & { id: string })[]>(() =>
				timeZoneOptions(current.value).map((o) => ({ ...o, id: o.value })),
			);

			const save = async (timezone: unknown) => {
				if (typeof timezone !== 'string' || timezone === current.value) return;
				saving.value = true;
				error.value = '';
				try {
					const saved = await saveUserTimeZone(timezone);
					store.commit('setUserTimeZone', saved || timezone);
				} catch {
					error.value = t('settings.timezone.saveFailed');
				} finally {
					saving.value = false;
				}
			};

			return {
				current,
				detected,
				options,
				saving,
				error,
				save,
				onSelect: save,
			};
		},
	});
</script>
