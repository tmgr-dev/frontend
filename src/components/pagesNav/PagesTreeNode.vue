<template>
	<div>
		<div
			class="group/page flex items-center gap-0.5 rounded-md pr-1 text-sm"
			:class="[
				onPage
					? 'text-ink hover:bg-surface-hover'
					: 'text-sidebar-foreground hover:bg-sidebar-accent hover:text-sidebar-accent-foreground',
				isActive
					? onPage
						? 'bg-surface-hover font-medium'
						: 'bg-sidebar-accent font-medium text-sidebar-accent-foreground'
					: '',
				flat ? 'pl-2' : '',
				zone.dragging.value ? 'ring-2 ring-brand' : '',
			]"
			:data-page-id="node.id"
			v-on="zone.handlers"
		>
			<button
				v-if="!flat && node.children.length"
				type="button"
				class="flex h-7 w-6 shrink-0 items-center justify-center rounded text-ink-subtle hover:text-ink"
				:aria-label="isExpanded ? 'Collapse' : 'Expand'"
				@click.stop="ctx.toggle(node.id)"
			>
				<ChevronRight
					class="h-3.5 w-3.5 transition-transform"
					:class="isExpanded ? 'rotate-90' : ''"
				/>
			</button>
			<span
				v-else-if="!flat"
				class="h-7 w-6 shrink-0"
				aria-hidden="true"
			></span>
			<router-link
				:to="url"
				draggable="false"
				class="flex h-7 min-w-0 flex-1 items-center gap-1.5"
			>
				<Pin v-if="node.pinned" class="h-3 w-3 shrink-0 text-ink-subtle" />
				<span class="truncate">{{ node.title }}</span>
			</router-link>
			<DropdownMenu>
				<DropdownMenuTrigger as-child>
					<button type="button" :class="actionsClass" aria-label="Page actions">
						<MoreHorizontal class="h-4 w-4" />
					</button>
				</DropdownMenuTrigger>
				<DropdownMenuContent align="start" class="w-52">
					<DropdownMenuSub>
						<DropdownMenuSubTrigger>
							<FilePlus class="mr-2 h-4 w-4" />
							<span>Add subpage</span>
						</DropdownMenuSubTrigger>
						<DropdownMenuSubContent>
							<DropdownMenuItem
								v-for="option in createOptions"
								:key="option.type"
								@select="ctx.createChild(node, option.type)"
							>
								{{ option.label }}
							</DropdownMenuItem>
						</DropdownMenuSubContent>
					</DropdownMenuSub>
					<DropdownMenuItem v-if="isDesktop" @select="openInWindow">
						<ExternalLink class="mr-2 h-4 w-4" />
						<span>Open in window</span>
					</DropdownMenuItem>
					<PageShareItems
						:page="node"
						:workspace-code="ctx.workspaceCode.value"
						submenu
						open-in-app
					/>
					<DropdownMenuItem @select="ctx.requestRename(node)">
						<Pencil class="mr-2 h-4 w-4" />
						<span>Rename</span>
					</DropdownMenuItem>
					<DropdownMenuItem @select="ctx.togglePin(node)">
						<component :is="node.pinned ? PinOff : Pin" class="mr-2 h-4 w-4" />
						<span>{{ node.pinned ? 'Unpin' : 'Pin' }}</span>
					</DropdownMenuItem>
					<DropdownMenuItem
						data-testid="page-export-subtree"
						@select="ctx.exportSubtree(node)"
					>
						<Download class="mr-2 h-4 w-4" />
						<span>Export with subpages</span>
					</DropdownMenuItem>
					<DropdownMenuItem
						data-testid="page-import-here"
						@select="ctx.importInto(node)"
					>
						<Upload class="mr-2 h-4 w-4" />
						<span>Import Markdown here…</span>
					</DropdownMenuItem>
					<DropdownMenuSeparator />
					<DropdownMenuItem
						class="text-destructive focus:text-destructive"
						@select="ctx.requestDelete(node)"
					>
						<Archive class="mr-2 h-4 w-4" />
						<span>Archive</span>
					</DropdownMenuItem>
				</DropdownMenuContent>
			</DropdownMenu>
		</div>
		<Draggable
			v-if="!flat && ((isExpanded && node.children.length) || showDropZone)"
			:model-value="node.children"
			group="pages"
			item-key="id"
			class="ml-3 min-h-[8px] border-l pl-1"
			:class="onPage ? 'border-line' : 'border-sidebar-border'"
			:data-parent-id="node.id"
			:force-fallback="true"
			:fallback-on-body="true"
			:fallback-tolerance="3"
			:delay="150"
			:delay-on-touch-only="true"
			@start="ctx.onDragStart"
			@end="ctx.dragging.value = null"
			@change="(event) => ctx.onChange(node.id, event)"
		>
			<template #item="{ element }">
				<PagesTreeNode :node="element" />
			</template>
		</Draggable>
	</div>
</template>

<script lang="ts">
	import PageShareItems from '@/components/pages/PageShareItems.vue';
	import {
		DropdownMenu,
		DropdownMenuContent,
		DropdownMenuItem,
		DropdownMenuSeparator,
		DropdownMenuSub,
		DropdownMenuSubContent,
		DropdownMenuSubTrigger,
		DropdownMenuTrigger,
	} from '@/components/ui/dropdown-menu';
	import { useFileDrop } from '@/composable/usePagesMarkdownIo';
	import { isDesktopApp } from '@/utils/desktop';
	import { openPageWindow, pageWindowTarget } from '@/utils/pageWindow';
	import {
		PAGE_CREATE_OPTIONS,
		pageUrl,
		type PageNode,
	} from '@/utils/pagesTree';
	import {
		Archive,
		ChevronRight,
		Download,
		ExternalLink,
		FilePlus,
		MoreHorizontal,
		Pencil,
		Pin,
		PinOff,
		Upload,
	} from 'lucide-vue-next';
	import { computed, defineComponent, inject, type PropType } from 'vue';
	import { useRoute } from 'vue-router';
	import Draggable from 'vuedraggable';
	import { PAGES_TREE_KEY, type PagesTreeContext } from './context';

	export default defineComponent({
		name: 'PagesTreeNode',
		components: {
			Archive,
			ChevronRight,
			DropdownMenu,
			DropdownMenuContent,
			DropdownMenuItem,
			DropdownMenuSeparator,
			DropdownMenuSub,
			DropdownMenuSubContent,
			DropdownMenuSubTrigger,
			DropdownMenuTrigger,
			Download,
			Draggable,
			ExternalLink,
			FilePlus,
			MoreHorizontal,
			PageShareItems,
			Pencil,
			Pin,
			PinOff,
			Upload,
		},
		props: {
			node: { type: Object as PropType<PageNode>, required: true },
			flat: { type: Boolean, default: false },
		},
		setup(props) {
			const ctx = inject(PAGES_TREE_KEY) as PagesTreeContext;
			// Not a static class: radix-vue's as-child Slot deletes `class` from the child's props, and static props are shared by every row.
			const actionsClass =
				'flex h-6 w-6 shrink-0 items-center justify-center rounded text-ink-subtle opacity-0 hover:bg-surface-active hover:text-ink focus-visible:opacity-100 group-focus-within/page:opacity-100 group-hover/page:opacity-100 data-[state=open]:opacity-100 [@media(hover:none)]:opacity-100';
			const route = useRoute();
			const isExpanded = computed(() => ctx.expanded.value.has(props.node.id));
			const showDropZone = computed(
				() =>
					ctx.dragging.value !== null &&
					ctx.dragging.value !== props.node.id &&
					!props.node.children.length,
			);
			const isActive = computed(() => route.params.slug === props.node.slug);
			const zone = useFileDrop((files) => ctx.importInto(props.node, files));
			const url = computed(() =>
				pageUrl(ctx.workspaceCode.value, props.node.slug),
			);

			const openInWindow = () => {
				const target = pageWindowTarget(props.node, ctx.workspaceCode.value);
				if (target) void openPageWindow(target).catch(() => undefined);
			};

			return {
				ctx,
				actionsClass,
				onPage: ctx.surface === 'page',
				zone,
				isDesktop: isDesktopApp(),
				openInWindow,
				isExpanded,
				showDropZone,
				isActive,
				url,
				createOptions: PAGE_CREATE_OPTIONS,
				Pin,
				PinOff,
			};
		},
	});
</script>
