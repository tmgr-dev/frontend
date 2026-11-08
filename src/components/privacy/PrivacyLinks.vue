<template>
	<p
		class="flex items-center justify-center gap-4 text-xs text-muted-foreground"
	>
		<router-link to="/privacy" class="hover:text-foreground hover:underline">
			{{ $t('settingsPages.privacyLinks.privacy') }}
		</router-link>
		<router-link to="/docs" class="hover:text-foreground hover:underline">
			{{ $t('settingsPages.privacyLinks.docs') }}
		</router-link>
		<router-link to="/docs/cli" class="hover:text-foreground hover:underline">
			{{ $t('settingsPages.privacyLinks.commandLine') }}
		</router-link>
		<button
			v-if="!desktop"
			type="button"
			class="hover:text-foreground hover:underline"
			@click="reopenChoice"
		>
			{{ $t('settingsPages.privacyLinks.cookieSettings') }}
		</button>
	</p>
</template>

<script setup lang="ts">
	import { useConsent } from '@/privacy/useConsent';
	import { isDesktopApp } from '@/utils/desktop';

	const { reopenChoice } = useConsent();
	const desktop = import.meta.env.MODE === 'desktop' || isDesktopApp();
</script>
