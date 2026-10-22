<template>
	<SidebarMenuButton
		v-if="variant === 'sidebar'"
		variant="outline"
		:tooltip="`Search (${hint})`"
		aria-label="Search"
		:aria-keyshortcuts="shortcut"
		data-testid="command-palette-trigger"
		class="text-ink-subtle dark:text-ink-subtle"
		@click="openCommandPalette"
	>
		<Search />
		<span>Search…</span>
		<span
			class="ml-auto text-xs tracking-widest text-muted-foreground group-data-[collapsible=icon]:hidden dark:text-muted-foreground"
			aria-hidden="true"
			>{{ hint }}</span
		>
	</SidebarMenuButton>
	<button
		v-else
		type="button"
		class="-ml-1 flex h-7 w-7 items-center justify-center rounded-md text-ink-subtle hover:bg-accent hover:text-accent-foreground dark:text-ink-subtle dark:hover:bg-accent"
		aria-label="Search"
		:aria-keyshortcuts="shortcut"
		:title="`Search (${hint})`"
		data-testid="command-palette-trigger"
		@click="openCommandPalette"
	>
		<Search class="h-4 w-4" />
	</button>
</template>

<script lang="ts">
	import { SidebarMenuButton } from '@/components/ui/sidebar';
	import { hotkeyHint, openCommandPalette } from '@/utils/commandPalette';
	import { desktopPlatform } from '@/utils/desktop';
	import { Search } from 'lucide-vue-next';
	import { defineComponent, type PropType } from 'vue';

	export default defineComponent({
		name: 'GlobalSearch',
		components: { Search, SidebarMenuButton },
		props: {
			variant: {
				type: String as PropType<'icon' | 'sidebar'>,
				default: 'icon',
			},
		},
		setup() {
			const isMac = desktopPlatform() === 'macos';
			return {
				openCommandPalette,
				hint: hotkeyHint(isMac),
				shortcut: isMac ? 'Meta+K' : 'Control+K',
			};
		},
	});
</script>
