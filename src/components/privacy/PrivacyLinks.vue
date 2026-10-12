<template>
	<p class="flex items-center justify-center gap-4 text-xs text-muted-foreground">
		<router-link to="/privacy" class="hover:text-foreground hover:underline">
			Privacy
		</router-link>
		<button
			v-if="!desktop"
			type="button"
			class="hover:text-foreground hover:underline"
			@click="reopenChoice"
		>
			Cookie settings
		</button>
	</p>
</template>

<script setup lang="ts">
	import { useConsent } from '@/privacy/useConsent';
	import { isDesktopApp } from '@/utils/desktop';

	const { reopenChoice } = useConsent();
	const desktop = import.meta.env.MODE === 'desktop' || isDesktopApp();
</script>
