<template>
	<section
		v-if="inline || visible"
		role="region"
		aria-label="Cookies and analytics"
		data-consent-panel
		:class="
			inline
				? 'rounded-lg border border-border bg-card p-4 text-card-foreground'
				: 'fixed inset-x-0 bottom-0 z-[60] border-t border-border bg-card p-4 pb-[max(1rem,env(safe-area-inset-bottom))] text-card-foreground shadow-lg sm:inset-x-auto sm:bottom-4 sm:right-4 sm:max-w-md sm:rounded-lg sm:border sm:pb-4'
		"
	>
		<h2 class="text-base font-semibold">Cookies and analytics</h2>
		<p class="mt-2 text-sm text-muted-foreground">
			We'd like to use Google Analytics to understand how people find and use
			TMGR. It sets cookies and sends data to Google. It stays off unless you
			allow it. You can change this any time.
		</p>
		<p class="mt-2 text-sm">
			<router-link
				to="/privacy#analytics"
				class="text-primary underline-offset-4 hover:underline"
			>
				Privacy policy
			</router-link>
		</p>
		<div class="mt-4 grid grid-cols-2 gap-3">
			<Button
				:variant="CONSENT_PANEL_BUTTON_VARIANT"
				data-consent="decline"
				@click="setAnalytics('denied')"
			>
				Decline
			</Button>
			<Button
				:variant="CONSENT_PANEL_BUTTON_VARIANT"
				data-consent="accept"
				@click="setAnalytics('granted')"
			>
				Allow analytics
			</Button>
		</div>
	</section>
</template>

<script setup lang="ts">
	import { Button } from '@/components/ui/button';
	import {
		CONSENT_PANEL_BUTTON_VARIANT,
		shouldShowConsentPanel,
	} from '@/privacy/panelVisibility';
	import { useConsent } from '@/privacy/useConsent';
	import { isDesktopApp } from '@/utils/desktop';
	import { computed } from 'vue';
	import { useRoute } from 'vue-router';

	defineProps<{ inline?: boolean }>();

	const route = useRoute();
	const { consent, setAnalytics } = useConsent();
	const desktop = import.meta.env.MODE === 'desktop' || isDesktopApp();

	const visible = computed(() =>
		shouldShowConsentPanel({
			analytics: consent.value.analytics,
			path: route.path,
			desktop,
		}),
	);
</script>
