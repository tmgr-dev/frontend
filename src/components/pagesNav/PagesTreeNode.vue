<template>
	<div>
		<div
			class="group/page flex items-center gap-0.5 rounded-md pr-1 text-sm text-sidebar-foreground hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
			:class="
				isActive
					? 'bg-sidebar-accent font-medium text-sidebar-accent-foreground'
					: ''
			"
			:data-page-id="node.id"
		>
			<button
				type="button"
				class="flex h-7 w-6 shrink-0 items-center justify-center rounded text-ink-subtle hover:text-ink dark:text-gray-400 dark:hover:text-gray-200"
				:aria-label="isExpanded ? 'Collapse' : 'Expand'"
				@click.stop="ctx.toggle(node.id)"
			>
				<ChevronRight
					class="h-3.5 w-3.5 transition-transform"
					:class="isExpanded ? 'rotate-90' : ''"
				/>
			</button>
			<router-link
				:to="url"
				draggable="false"
				class="flex h-7 min-w-0 flex-1 items-center gap-1.5"
			>
				<Pin
					v-if="node.pinned"
					class="h-3 w-3 shrink-0 text-ink-subtle dark:text-gray-400"
				/>
				<span class="truncate">{{ node.title }}</span>
			</router-link>
			<DropdownMenu>
				<DropdownMenuTrigger as-child>
					<button
						type="button"
						class="flex h-6 w-6 shrink-0 items-center justify-center rounded text-ink-subtle opacity-0 hover:bg-black/5 hover:text-ink focus-visible:opacity-100 group-hover/page:opacity-100 data-[state=open]:opacity-100 dark:text-gray-400 dark:hover:bg-white/10 dark:hover:text-gray-200"
						aria-label="Page actions"
					>
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
					<DropdownMenuItem @select="ctx.requestRename(node)">
						<Pencil class="mr-2 h-4 w-4" />
						<span>Rename</span>
					</DropdownMenuItem>
					<DropdownMenuItem @select="ctx.togglePin(node)">
						<component :is="node.pinned ? PinOff : Pin" class="mr-2 h-4 w-4" />
						<span>{{ node.pinned ? 'Unpin' : 'Pin' }}</span>
					</DropdownMenuItem>
					<DropdownMenuSeparator />
					<DropdownMenuItem
						class="text-destructive focus:text-destructive"
						@select="ctx.requestDelete(node)"
					>
						<Trash2 class="mr-2 h-4 w-4" />
						<span>Delete</span>
					</DropdownMenuItem>
				</DropdownMenuContent>
			</DropdownMenu>
		</div>
		<Draggable
			v-if="isExpanded"
			:model-value="node.children"
			group="pages"
			item-key="id"
			class="ml-3 min-h-[8px] border-l border-sidebar-border pl-1 dark:border-gray-700"
			:data-parent-id="node.id"
			:force-fallback="true"
			:fallback-on-body="true"
			:fallback-tolerance="3"
			:delay="150"
			:delay-on-touch-only="true"
			@change="(event) => ctx.onChange(node.id, event)"
		>
			<template #item="{ element }">
				<PagesTreeNode :node="element" />
			</template>
		</Draggable>
	</div>
</template>

<script lang="ts">
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
	import { isDesktopApp } from '@/utils/desktop';
	import { openPageWindow, pageWindowTarget } from '@/utils/pageWindow';
	import {
		PAGE_CREATE_OPTIONS,
		pageUrl,
		type PageNode,
	} from '@/utils/pagesTree';
	import {
		ChevronRight,
		ExternalLink,
		FilePlus,
		MoreHorizontal,
		Pencil,
		Pin,
		PinOff,
		Trash2,
	} from 'lucide-vue-next';
	import { computed, defineComponent, inject, type PropType } from 'vue';
	import { useRoute } from 'vue-router';
	import Draggable from 'vuedraggable';
	import { PAGES_TREE_KEY, type PagesTreeContext } from './context';

	export default defineComponent({
		name: 'PagesTreeNode',
		components: {
			ChevronRight,
			DropdownMenu,
			DropdownMenuContent,
			DropdownMenuItem,
			DropdownMenuSeparator,
			DropdownMenuSub,
			DropdownMenuSubContent,
			DropdownMenuSubTrigger,
			DropdownMenuTrigger,
			Draggable,
			ExternalLink,
			FilePlus,
			MoreHorizontal,
			Pencil,
			Pin,
			PinOff,
			Trash2,
		},
		props: {
			node: { type: Object as PropType<PageNode>, required: true },
		},
		setup(props) {
			const ctx = inject(PAGES_TREE_KEY) as PagesTreeContext;
			const route = useRoute();
			const isExpanded = computed(() => ctx.expanded.value.has(props.node.id));
			const isActive = computed(() => route.params.slug === props.node.slug);
			const url = computed(() => pageUrl(ctx.workspaceCode.value, props.node.slug));

			const openInWindow = () => {
				const target = pageWindowTarget(props.node, ctx.workspaceCode.value);
				if (target) void openPageWindow(target).catch(() => undefined);
			};

			return {
				ctx,
				isDesktop: isDesktopApp(),
				openInWindow,
				isExpanded,
				isActive,
				url,
				createOptions: PAGE_CREATE_OPTIONS,
				Pin,
				PinOff,
			};
		},
	});
</script>
