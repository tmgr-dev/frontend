<template>
	<PageContainer width="narrow">
		<RequiresAccount v-if="mode === 'requires-account'" :module="title" />
		<div
			v-else
			class="flex flex-col items-center gap-3 rounded-lg border border-border bg-card p-8 text-center text-card-foreground"
		>
			<h1 class="text-lg font-semibold text-ink">{{ title }}</h1>
			<p class="text-sm text-ink-subtle">
				{{
					mode === 'not-in-local-workspace'
						? 'Not available in local workspaces'
						: 'This module is off.'
				}}
			</p>
			<router-link
				v-if="mode === 'default' && copy.kind === 'link'"
				:to="copy.to"
				class="inline-flex items-center justify-center rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:opacity-90"
			>
				{{ copy.text }}
			</router-link>
			<p v-else-if="mode === 'default'" class="text-sm text-ink-subtle">
				{{ copy.text }}
			</p>
		</div>
	</PageContainer>
</template>

<script setup lang="ts">
	import RequiresAccount from '@/components/general/RequiresAccount.vue';
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import { gateMode } from '@/utils/localOnly';
	import { gateCopy } from '@/utils/modules';
	import { computed } from 'vue';
	import { useStore } from 'vuex';

	const props = defineProps<{ title: string; featureKey: string }>();

	const store = useStore();
	const mode = computed(() =>
		gateMode({
			entry: store.getters['featureToggles/moduleEntry'](props.featureKey),
			localOnly: store.getters.isLocalOnly === true,
			localWorkspace: store.getters.currentWorkspace?.is_local === true,
		}),
	);
	const copy = computed(() =>
		gateCopy(
			store.getters['featureToggles/canManageModules'],
			store.getters['featureToggles/isHiddenByMe'](props.featureKey),
		),
	);
</script>
