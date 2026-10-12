<template>
	<section
		v-if="inline || visible"
		role="region"
		aria-label="Privacy choices"
		data-consent-panel
		:class="
			inline
				? 'rounded-lg border border-border bg-card p-4 text-card-foreground'
				: 'fixed inset-x-0 bottom-0 z-[60] max-h-[90vh] overflow-y-auto border-t border-border bg-card p-4 pb-[max(1rem,env(safe-area-inset-bottom))] text-card-foreground shadow-lg sm:inset-x-auto sm:bottom-4 sm:right-4 sm:max-w-md sm:rounded-lg sm:border sm:pb-4'
		"
	>
		<div v-if="showUsage" data-consent-block="usage">
			<h2 class="text-base font-semibold">Help improve TMGR?</h2>
			<p class="mt-2 text-sm text-muted-foreground">
				If you agree, TMGR sends usage events that are not linked to your
				account: which screens and tours you open, which steps you finish, app
				version and platform.
			</p>
			<dl class="mt-2 space-y-1 text-sm">
				<div>
					<dt class="inline font-medium">We send:</dt>
					<dd class="inline text-muted-foreground">
						event names like tour_step_completed · a random install ID · app
						version, OS, screen size class
					</dd>
				</div>
				<div>
					<dt class="inline font-medium">We never send:</dt>
					<dd class="inline text-muted-foreground">
						task, page or comment text · names, emails, file names · your
						account ID. Your IP address is not stored with events.
					</dd>
				</div>
			</dl>
			<p class="mt-2 text-sm text-muted-foreground">
				Off by default. Change it any time in Settings → Privacy. Turning it off
				stops sending at once; you can also delete what was already sent.
			</p>
			<p class="mt-2 text-sm">
				<router-link
					to="/privacy#usage-data"
					class="text-primary underline-offset-4 hover:underline"
				>
					What we collect
				</router-link>
			</p>
			<div class="mt-4 grid grid-cols-2 gap-3">
				<Button
					:variant="CONSENT_PANEL_BUTTON_VARIANT"
					data-consent="usage-decline"
					@click="setUsage('denied')"
				>
					No, thanks
				</Button>
				<Button
					:variant="CONSENT_PANEL_BUTTON_VARIANT"
					data-consent="usage-accept"
					@click="setUsage('granted')"
				>
					Share usage data
				</Button>
			</div>
		</div>
		<div
			v-if="showAnalytics"
			data-consent-block="analytics"
			:class="showUsage && 'mt-4 border-t border-border pt-4'"
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
		</div>
	</section>
</template>

<script setup lang="ts">
	import { Button } from '@/components/ui/button';
	import {
		CONSENT_PANEL_BUTTON_VARIANT,
		shouldShowConsentPanel,
		showAnalyticsBlock,
		showUsageBlock,
	} from '@/privacy/panelVisibility';
	import { useConsent } from '@/privacy/useConsent';
	import { isDesktopApp } from '@/utils/desktop';
	import { computed } from 'vue';
	import { useRoute } from 'vue-router';

	const props = defineProps<{ inline?: boolean }>();

	const route = useRoute();
	const { consent, setAnalytics, setUsage } = useConsent();
	const desktop = import.meta.env.MODE === 'desktop' || isDesktopApp();

	const state = computed(() => ({
		analytics: consent.value.analytics,
		usage: consent.value.usage,
		path: route.path,
		desktop,
	}));
	const visible = computed(() => shouldShowConsentPanel(state.value));
	const showAnalytics = computed(() =>
		props.inline ? !desktop : showAnalyticsBlock(state.value),
	);
	const showUsage = computed(() => !props.inline && showUsageBlock(state.value));
</script>
