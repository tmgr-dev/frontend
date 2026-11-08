<template>
	<div class="flex flex-col gap-6">
		<UsageDataControls data-tour="settings.privacy-telemetry" />

		<AiUsageSettings v-if="desktop" />

		<SettingsSection
			v-if="!desktop"
			:title="$t('settingsPages.privacySettings.cookiesTitle')"
		>
			<SettingsRow
				v-slot="{ labelId, descriptionId }"
				:label="$t('settingsPages.privacySettings.googleAnalytics')"
				:description="
					$t('settingsPages.privacySettings.googleAnalyticsDescription')
				"
			>
				<Switch
					data-testid="analytics-switch"
					:checked="consent.analytics === 'granted'"
					:aria-labelledby="labelId"
					:aria-describedby="descriptionId"
					@update:checked="
						(value) => setAnalytics(value ? 'granted' : 'denied')
					"
				/>
			</SettingsRow>
		</SettingsSection>

		<router-link to="/privacy" class="text-sm text-primary hover:underline">
			{{ $t('settingsPages.privacySettings.privacyPolicy') }}
		</router-link>
	</div>
</template>

<script setup lang="ts">
	import SettingsRow from '@/components/layouts/SettingsRow.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import UsageDataControls from '@/components/privacy/UsageDataControls.vue';
	import AiUsageSettings from '@/components/usage/AiUsageSettings.vue';
	import { Switch } from '@/components/ui/switch';
	import { useConsent } from '@/privacy/useConsent';
	import { isDesktopApp } from '@/utils/desktop';

	const { consent, setAnalytics } = useConsent();
	const desktop = import.meta.env.MODE === 'desktop' || isDesktopApp();
</script>
