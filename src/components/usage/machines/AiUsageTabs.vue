<template>
	<nav
		class="inline-flex rounded-md border border-border p-0.5"
		:aria-label="$t('agents.usage.machines.tabs.label')"
		data-testid="ai-usage-tabs"
	>
		<router-link
			v-for="tab in tabs"
			:key="tab.to"
			:to="tab.to"
			class="rounded px-3 py-1 text-sm"
			:class="
				current === tab.key
					? 'bg-muted font-medium text-ink'
					: 'text-ink-subtle hover:text-ink'
			"
			:aria-current="current === tab.key ? 'page' : undefined"
		>
			{{ tab.label }}
		</router-link>
	</nav>
</template>

<script lang="ts">
	import { t } from '@/i18n';
	import { computed, defineComponent, type PropType } from 'vue';

	export default defineComponent({
		name: 'AiUsageTabs',
		props: {
			current: {
				type: String as PropType<'device' | 'all'>,
				required: true,
			},
		},
		setup() {
			return {
				tabs: computed(() => [
					{
						key: 'device',
						to: '/ai-usage',
						label: t('agents.usage.machines.tabs.device'),
					},
					{
						key: 'all',
						to: '/ai-usage/machines',
						label: t('agents.usage.machines.tabs.all'),
					},
				]),
			};
		},
	});
</script>
