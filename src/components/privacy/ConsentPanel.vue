<template>
	<section
		v-if="inline || visible || saveFailed"
		role="region"
		:aria-label="$t('settingsPages.consentPanel.ariaLabel')"
		data-consent-panel
		:class="
			inline
				? 'rounded-lg border border-border bg-card p-4 text-card-foreground'
				: 'fixed inset-x-0 bottom-0 z-[60] max-h-[90vh] overflow-y-auto border-t border-border bg-card p-4 pb-[max(1rem,env(safe-area-inset-bottom))] text-card-foreground shadow-lg sm:inset-x-auto sm:right-4 sm:bottom-4 sm:max-w-md sm:rounded-lg sm:border sm:pb-4' +
					(authPage ? ' lg:right-auto lg:left-4' : '')
		"
	>
		<p v-if="saveFailed" role="alert" class="mb-3 text-sm text-destructive">
			{{ $t('settingsPages.consentPanel.saveFailed') }}
		</p>
		<div v-if="showUsage" data-consent-block="usage">
			<h2 class="text-base font-semibold">
				{{ $t('settingsPages.consentPanel.usage.title') }}
			</h2>
			<p class="mt-2 text-sm text-muted-foreground">
				{{ $t('settingsPages.consentPanel.usage.intro') }}
			</p>
			<p class="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 text-sm">
				<button
					type="button"
					class="text-primary underline-offset-4 hover:underline"
					:aria-expanded="detailsOpen"
					aria-controls="usage-consent-details"
					data-consent="usage-details"
					@click="detailsOpen = !detailsOpen"
				>
					{{ $t('settingsPages.consentPanel.usage.whatWeSend') }}
				</button>
				<router-link
					to="/privacy#usage-data"
					class="text-primary underline-offset-4 hover:underline"
				>
					{{ $t('settingsPages.consentPanel.usage.whatWeCollect') }}
				</router-link>
			</p>
			<div v-if="detailsOpen" id="usage-consent-details" class="mt-2">
				<p class="text-sm text-muted-foreground">
					{{ $t('settingsPages.consentPanel.usage.details') }}
				</p>
				<dl class="space-y-1 mt-2 text-sm">
					<div>
						<dt class="inline font-medium">
							{{ $t('settingsPages.consentPanel.usage.weSend') }}
						</dt>
						<dd class="inline text-muted-foreground">
							{{ $t('settingsPages.consentPanel.usage.weSendText') }}
						</dd>
					</div>
					<div>
						<dt class="inline font-medium">
							{{ $t('settingsPages.consentPanel.usage.weNeverSend') }}
						</dt>
						<dd class="inline text-muted-foreground">
							{{ $t('settingsPages.consentPanel.usage.weNeverSendText') }}
						</dd>
					</div>
				</dl>
				<p class="mt-2 text-sm text-muted-foreground">
					{{ $t('settingsPages.consentPanel.usage.changeAnytime') }}
				</p>
			</div>
			<div class="mt-4 grid grid-cols-2 gap-3">
				<Button
					:variant="CONSENT_PANEL_BUTTON_VARIANT"
					data-consent="usage-decline"
					@click="setUsage('denied')"
				>
					{{ $t('settingsPages.consentPanel.usage.decline') }}
				</Button>
				<Button
					:variant="CONSENT_PANEL_BUTTON_VARIANT"
					data-consent="usage-accept"
					@click="setUsage('granted')"
				>
					{{ $t('settingsPages.consentPanel.usage.accept') }}
				</Button>
			</div>
		</div>
		<div
			v-if="showAnalytics"
			data-consent-block="analytics"
			:class="showUsage && 'mt-4 border-t border-border pt-4'"
		>
			<h2 class="text-base font-semibold">
				{{ $t('settingsPages.consentPanel.analytics.title') }}
			</h2>
			<p class="mt-2 text-sm text-muted-foreground">
				{{ $t('settingsPages.consentPanel.analytics.text') }}
			</p>
			<p class="mt-2 text-sm">
				<router-link
					to="/privacy#analytics"
					class="text-primary underline-offset-4 hover:underline"
				>
					{{ $t('settingsPages.consentPanel.analytics.privacyPolicy') }}
				</router-link>
			</p>
			<div class="mt-4 grid grid-cols-2 gap-3">
				<Button
					:variant="CONSENT_PANEL_BUTTON_VARIANT"
					data-consent="decline"
					@click="setAnalytics('denied')"
				>
					{{ $t('settingsPages.consentPanel.analytics.decline') }}
				</Button>
				<Button
					:variant="CONSENT_PANEL_BUTTON_VARIANT"
					data-consent="accept"
					@click="setAnalytics('granted')"
				>
					{{ $t('settingsPages.consentPanel.analytics.accept') }}
				</Button>
			</div>
		</div>
	</section>
</template>

<script setup lang="ts">
	import { Button } from '@/components/ui/button';
	import {
		CONSENT_PANEL_BUTTON_VARIANT,
		isAuthRoute,
		shouldShowConsentPanel,
		showAnalyticsBlock,
		showUsageBlock,
	} from '@/privacy/panelVisibility';
	import { useConsent } from '@/privacy/useConsent';
	import store from '@/store';
	import { isDesktopApp } from '@/utils/desktop';
	import { computed, ref } from 'vue';
	import { useRoute } from 'vue-router';

	const props = defineProps<{ inline?: boolean }>();

	const route = useRoute();
	const { consent, setAnalytics, setUsage, saveFailed } = useConsent();
	const desktop = import.meta.env.MODE === 'desktop' || isDesktopApp();

	const detailsOpen = ref(false);
	const authPage = computed(() => isAuthRoute(route.name));

	const state = computed(() => ({
		analytics: consent.value.analytics,
		usage: consent.value.usage,
		path: route.path,
		desktop,
		ready: store.getters.canUseApp,
	}));
	const visible = computed(() => shouldShowConsentPanel(state.value));
	const showAnalytics = computed(() =>
		props.inline ? !desktop : showAnalyticsBlock(state.value),
	);
	const showUsage = computed(
		() => !props.inline && showUsageBlock(state.value),
	);
</script>
