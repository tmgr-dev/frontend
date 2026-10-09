<template>
	<PageContainer width="narrow">
		<div
			class="flex flex-col items-center gap-3 rounded-lg border border-border bg-card p-8 text-center text-card-foreground"
		>
			<h1 class="text-lg font-semibold text-ink">{{ title }}</h1>
			<p class="text-sm text-ink-subtle">This module is off.</p>
			<router-link
				v-if="copy.kind === 'link'"
				:to="copy.to"
				class="inline-flex items-center justify-center rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:opacity-90"
			>
				{{ copy.text }}
			</router-link>
			<p v-else class="text-sm text-ink-subtle">{{ copy.text }}</p>
		</div>
	</PageContainer>
</template>

<script setup lang="ts">
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import { gateCopy } from '@/utils/modules';
	import { computed } from 'vue';
	import { useStore } from 'vuex';

	const props = defineProps<{ title: string; featureKey: string }>();

	const store = useStore();
	const copy = computed(() =>
		gateCopy(
			store.getters['featureToggles/canManageModules'],
			store.getters['featureToggles/isHiddenByMe'](props.featureKey),
		),
	);
</script>
