<template>
	<div class="flex flex-col gap-6">
		<UsageDataControls data-tour="settings.privacy-telemetry" />

		<SettingsSection v-if="!desktop" title="Cookies and analytics">
			<SettingsRow
				v-slot="{ labelId, descriptionId }"
				label="Google Analytics"
				description="Helps us understand how TMGR is used. Sets cookies and sends data to Google. Off unless you allow it."
			>
				<Switch
					data-testid="analytics-switch"
					:checked="consent.analytics === 'granted'"
					:aria-labelledby="labelId"
					:aria-describedby="descriptionId"
					@update:checked="(value) => setAnalytics(value ? 'granted' : 'denied')"
				/>
			</SettingsRow>
		</SettingsSection>

		<router-link to="/privacy" class="text-sm text-primary hover:underline">
			Privacy policy
		</router-link>
	</div>
</template>

<script setup lang="ts">
	import SettingsRow from '@/components/layouts/SettingsRow.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import UsageDataControls from '@/components/privacy/UsageDataControls.vue';
	import { Switch } from '@/components/ui/switch';
	import { useConsent } from '@/privacy/useConsent';
	import { isDesktopApp } from '@/utils/desktop';

	const { consent, setAnalytics } = useConsent();
	const desktop = import.meta.env.MODE === 'desktop' || isDesktopApp();
</script>
