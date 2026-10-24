<template>
	<SidebarGroup
		class="group-data-[collapsible=icon]:hidden"
		:class="zone.dragging.value ? 'rounded-md ring-2 ring-brand' : ''"
		data-testid="pages-sidebar-section"
		data-pages-tree="sidebar"
		data-tour="pages.tree"
		v-on="zone.handlers"
	>
		<SidebarGroupLabel>Pages</SidebarGroupLabel>
		<div class="absolute top-3.5 right-3 flex items-center gap-0.5">
			<button
				type="button"
				class="flex h-5 w-5 items-center justify-center rounded text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
				title="New page"
				aria-label="New page"
				@click="createRoot('plain')"
			>
				<Plus class="h-4 w-4" />
			</button>
			<DropdownMenu>
				<DropdownMenuTrigger as-child>
					<button
						type="button"
						:class="typePickerClass"
						title="Page type"
						aria-label="Choose page type"
						data-tour="pages.type-picker"
					>
						<ChevronDown class="h-3.5 w-3.5" />
					</button>
				</DropdownMenuTrigger>
				<DropdownMenuContent align="end" class="w-48">
					<DropdownMenuItem
						v-for="option in createOptions"
						:key="option.type"
						@select="createRoot(option.type)"
					>
						{{ option.label }}
					</DropdownMenuItem>
					<DropdownMenuSeparator />
					<DropdownMenuItem
						data-testid="pages-sidebar-import"
						@select="io.openImport(null)"
					>
						<Upload class="mr-2 h-4 w-4" />
						<span>Import Markdown…</span>
					</DropdownMenuItem>
				</DropdownMenuContent>
			</DropdownMenu>
		</div>

		<div v-if="!loaded" class="px-2 py-1 text-xs text-sidebar-foreground/60">
			Loading…
		</div>
		<div
			v-else-if="failed && !pages.length"
			class="px-2 py-1 text-xs text-destructive"
		>
			Failed to load pages
		</div>
		<template v-else>
			<PagesTreeNode
				v-for="node in lists.pinned"
				:key="node.id"
				:node="node"
				flat
			/>
			<PagesTreeNode
				v-for="node in lists.recent"
				:key="node.id"
				:node="node"
				flat
			/>
		</template>

		<router-link
			:to="`/${workspaceCode}/pages`"
			class="mt-1 block rounded-md px-2 py-1 text-xs text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
		>
			All pages
		</router-link>
		<router-link
			:to="`/${workspaceCode}/pages/_/archive`"
			class="flex items-center gap-1.5 rounded-md px-2 py-1 text-xs text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
		>
			<Archive class="h-3 w-3" />
			Archive
		</router-link>

		<PagesTreeDialogs />
	</SidebarGroup>
</template>

<script lang="ts">
	import type { PageType } from '@/actions/tmgr/pages';
	import {
		DropdownMenu,
		DropdownMenuContent,
		DropdownMenuItem,
		DropdownMenuSeparator,
		DropdownMenuTrigger,
	} from '@/components/ui/dropdown-menu';
	import { SidebarGroup, SidebarGroupLabel } from '@/components/ui/sidebar';
	import { useFileDrop } from '@/composable/usePagesMarkdownIo';
	import { usePagesTreeHost } from '@/composable/usePagesTreeHost';
	import { PAGE_CREATE_OPTIONS, sidebarPages } from '@/utils/pagesTree';
	import { Archive, ChevronDown, Plus, Upload } from 'lucide-vue-next';
	import { computed, defineComponent, toRef } from 'vue';
	import PagesTreeDialogs from './PagesTreeDialogs.vue';
	import PagesTreeNode from './PagesTreeNode.vue';

	export default defineComponent({
		name: 'SidebarPagesSection',
		components: {
			ChevronDown,
			DropdownMenu,
			DropdownMenuContent,
			DropdownMenuItem,
			DropdownMenuSeparator,
			DropdownMenuTrigger,
			PagesTreeDialogs,
			PagesTreeNode,
			Plus,
			SidebarGroup,
			SidebarGroupLabel,
			Archive,
			Upload,
		},
		props: {
			workspaceId: { type: Number, required: true },
			workspaceCode: { type: String, required: true },
		},
		setup(props) {
			const { tree, actions, io } = usePagesTreeHost(
				toRef(props, 'workspaceId'),
				toRef(props, 'workspaceCode'),
				'sidebar',
			);
			const zone = useFileDrop((files) => io.openImport(null, files));
			// Not a static class: see PagesTreeNode actionsClass.
			const typePickerClass =
				'flex h-5 w-4 items-center justify-center rounded text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground';

			const createRoot = async (type: PageType) => {
				await actions.create(type, null);
			};

			return {
				lists: computed(() => sidebarPages(tree.pages.value)),
				pages: tree.pages,
				loaded: tree.loaded,
				failed: tree.failed,
				io,
				zone,
				createRoot,
				typePickerClass,
				createOptions: PAGE_CREATE_OPTIONS,
			};
		},
	});
</script>
