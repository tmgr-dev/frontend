<template>
	<SidebarGroup
		class="group-data-[collapsible=icon]:hidden"
		:class="zone.dragging.value ? 'rounded-md ring-2 ring-brand' : ''"
		data-testid="pages-sidebar-section"
		data-pages-tree="sidebar"
		data-tour="pages.tree"
		v-on="zone.handlers"
	>
		<SidebarGroupLabel>{{ $t('pages.nav.sidebar.title') }}</SidebarGroupLabel>
		<div class="absolute top-3.5 right-3 flex items-center gap-0.5">
			<button
				type="button"
				class="flex h-5 w-5 items-center justify-center rounded text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
				:title="$t('pages.nav.sidebar.newPage')"
				:aria-label="$t('pages.nav.sidebar.newPage')"
				@click="createRoot('plain')"
			>
				<Plus class="h-4 w-4" />
			</button>
			<DropdownMenu>
				<DropdownMenuTrigger as-child>
					<button
						type="button"
						:class="typePickerClass"
						:title="$t('pages.nav.sidebar.pageType')"
						:aria-label="$t('pages.nav.sidebar.choosePageType')"
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
						{{ $t(option.labelKey) }}
					</DropdownMenuItem>
					<DropdownMenuSeparator />
					<DropdownMenuItem
						data-testid="pages-sidebar-import"
						@select="io.openImport(null)"
					>
						<Upload class="mr-2 h-4 w-4" />
						<span>{{ $t('pages.nav.sidebar.importMarkdown') }}</span>
					</DropdownMenuItem>
				</DropdownMenuContent>
			</DropdownMenu>
		</div>

		<div v-if="!loaded" class="px-2 py-1 text-xs text-sidebar-foreground/60">
			{{ $t('common.states.loading') }}
		</div>
		<div
			v-else-if="failed && !pages.length"
			class="px-2 py-1 text-xs text-destructive"
		>
			{{ $t('pages.nav.sidebar.failed') }}
		</div>
		<template v-else>
			<template v-if="lists.pinned.length">
				<div :class="subLabelClass">{{ $t('pages.nav.sidebar.pinned') }}</div>
				<PagesTreeNode
					v-for="node in lists.pinned"
					:key="node.id"
					:node="node"
					flat
				/>
			</template>
			<template v-if="lists.recent.length">
				<div :class="subLabelClass">{{ $t('pages.nav.sidebar.recent') }}</div>
				<PagesTreeNode
					v-for="node in lists.recent"
					:key="node.id"
					:node="node"
					flat
				/>
			</template>
			<template v-if="pages.length">
				<div :class="subLabelClass">{{ $t('pages.nav.sidebar.tree') }}</div>
				<div
					class="max-h-[45vh] overflow-y-auto"
					data-testid="pages-sidebar-tree"
				>
					<Draggable
						:model-value="visibleRoots"
						group="pages"
						item-key="id"
						class="min-h-[8px]"
						:force-fallback="true"
						:fallback-on-body="true"
						:fallback-tolerance="3"
						:delay="150"
						:delay-on-touch-only="true"
						@start="onDragStart"
						@end="dragging = null"
						@change="(event) => onChange(null, event)"
					>
						<template #item="{ element }">
							<PagesTreeNode :node="element" />
						</template>
					</Draggable>
					<button
						v-if="hiddenRoots > 0"
						type="button"
						class="w-full rounded-md px-2 py-1 text-left text-xs text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
						data-testid="pages-sidebar-show-more"
						@click="shown += rootsPage"
					>
						{{ $t('pages.nav.sidebar.showMore', { count: hiddenRoots }) }}
					</button>
				</div>
			</template>
		</template>

		<router-link
			:to="`/${workspaceCode}/pages`"
			class="mt-1 block rounded-md px-2 py-1 text-xs text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
		>
			{{ $t('pages.nav.sidebar.allPages') }}
		</router-link>
		<router-link
			:to="`/${workspaceCode}/pages/_/archive`"
			class="flex items-center gap-1.5 rounded-md px-2 py-1 text-xs text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
		>
			<Archive class="h-3 w-3" />
			{{ $t('pages.nav.sidebar.archive') }}
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
	import {
		PAGE_CREATE_OPTIONS,
		SIDEBAR_ROOTS_PAGE,
		ancestorIds,
		sidebarPages,
		visibleRootCount,
	} from '@/utils/pagesTree';
	import { Archive, ChevronDown, Plus, Upload } from 'lucide-vue-next';
	import { computed, defineComponent, ref, toRef, watch } from 'vue';
	import { useRoute } from 'vue-router';
	import Draggable from 'vuedraggable';
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
			Draggable,
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
			const route = useRoute();
			const { tree, actions, io, dragging, onDragStart, onChange } =
				usePagesTreeHost(
					toRef(props, 'workspaceId'),
					toRef(props, 'workspaceCode'),
					'sidebar',
				);
			const zone = useFileDrop((files) => io.openImport(null, files));
			// Not a static class: see PagesTreeNode actionsClass.
			const typePickerClass =
				'flex h-5 w-4 items-center justify-center rounded text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground';

			const subLabelClass =
				'px-2 pt-1.5 pb-0.5 text-[11px] font-medium uppercase tracking-wide text-sidebar-foreground/50';
			const shown = ref(SIDEBAR_ROOTS_PAGE);
			const slug = computed(() => route.params.slug as string | undefined);
			const activeRootIndex = computed(() => {
				if (!slug.value) return -1;
				const ids = new Set(ancestorIds(tree.pages.value, slug.value));
				const active = tree.pages.value.find((p) => p.slug === slug.value);
				if (active) ids.add(active.id);
				return tree.tree.value.findIndex((root) => ids.has(root.id));
			});
			const visibleCount = computed(() =>
				visibleRootCount(
					tree.tree.value.length,
					shown.value,
					activeRootIndex.value,
				),
			);
			const visibleRoots = computed(() =>
				tree.tree.value.slice(0, visibleCount.value),
			);

			watch(
				[slug, tree.loaded],
				() => {
					if (!tree.loaded.value || !slug.value) return;
					const ids = ancestorIds(tree.pages.value, slug.value).filter(
						(id) => !tree.expanded.value.has(id),
					);
					if (ids.length) tree.expandMany(ids);
				},
				{ immediate: true },
			);

			watch(
				() => props.workspaceId,
				() => (shown.value = SIDEBAR_ROOTS_PAGE),
			);

			const createRoot = async (type: PageType) => {
				await actions.create(type, null);
			};

			return {
				subLabelClass,
				shown,
				rootsPage: SIDEBAR_ROOTS_PAGE,
				visibleRoots,
				hiddenRoots: computed(
					() => tree.tree.value.length - visibleCount.value,
				),
				dragging,
				onDragStart,
				onChange,
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
