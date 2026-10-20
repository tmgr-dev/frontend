<template>
	<div
		:class="
			hasColumn
				? 'mx-auto flex w-full min-w-0 max-w-7xl flex-1'
				: 'flex min-w-0 flex-1 flex-col'
		"
		:data-testid="hasColumn ? 'settings-layout' : undefined"
	>
		<aside v-if="hasColumn" class="w-52 shrink-0 pb-10 pl-6 pt-6">
			<SettingsNav class="sticky top-[calc(var(--titlebar-h,0px)+1.5rem)]" />
		</aside>
		<div v-else-if="!onSections" class="px-4 pt-4 md:px-6">
			<router-link
				to="/settings/sections"
				aria-label="Back to Settings"
				data-testid="settings-back"
				class="inline-flex items-center gap-1 text-sm text-ink-subtle hover:text-ink dark:text-ink-subtle dark:hover:text-ink"
			>
				<ChevronLeft class="h-4 w-4" />
				Settings
			</router-link>
		</div>
		<div class="flex min-w-0 flex-1 flex-col">
			<slot />
		</div>
	</div>
</template>

<script lang="ts">
	import { SETTINGS_NAV_COLUMN_QUERY } from '@/utils/settingsNav';
	import { useMediaQuery } from '@vueuse/core';
	import { ChevronLeft } from 'lucide-vue-next';
	import { computed, defineComponent, watch } from 'vue';
	import { useRoute, useRouter } from 'vue-router';
	import SettingsNav from './SettingsNav.vue';

	export default defineComponent({
		name: 'SettingsLayout',
		components: { ChevronLeft, SettingsNav },
		setup() {
			const route = useRoute();
			const router = useRouter();
			const hasColumn = useMediaQuery(SETTINGS_NAV_COLUMN_QUERY);
			const onSections = computed(() => route.name === 'SettingsSections');

			watch(
				() => hasColumn.value && onSections.value,
				(stranded) => {
					if (stranded) router.replace('/settings/workspaces');
				},
			);

			return { hasColumn, onSections };
		},
	});
</script>
