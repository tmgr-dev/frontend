<template>
	<SidebarGroup class="group-data-[collapsible=icon]:hidden">
		<SidebarGroupLabel>Страницы</SidebarGroupLabel>
		<div class="absolute right-3 top-3.5 flex items-center gap-0.5">
			<button
				type="button"
				class="flex h-5 w-5 items-center justify-center rounded text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
				title="Новая страница"
				aria-label="Новая страница"
				@click="createRoot('plain')"
			>
				<Plus class="h-4 w-4" />
			</button>
			<DropdownMenu>
				<DropdownMenuTrigger as-child>
					<button
						type="button"
						class="flex h-5 w-4 items-center justify-center rounded text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
						title="Тип страницы"
						aria-label="Выбрать тип страницы"
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
				</DropdownMenuContent>
			</DropdownMenu>
		</div>

		<div v-if="!loaded" class="px-2 py-1 text-xs text-sidebar-foreground/60">
			Загрузка…
		</div>
		<div
			v-else-if="failed && !pages.length"
			class="px-2 py-1 text-xs text-destructive"
		>
			Не удалось загрузить страницы
		</div>
		<Draggable
			v-else
			:model-value="tree"
			group="pages"
			item-key="id"
			class="min-h-[8px]"
			:force-fallback="true"
			:fallback-on-body="true"
			:fallback-tolerance="3"
			:delay="150"
			:delay-on-touch-only="true"
			@change="(event) => onChange(null, event)"
		>
			<template #item="{ element }">
				<PagesTreeNode :node="element" />
			</template>
		</Draggable>

		<router-link
			:to="`/${workspaceCode}/pages`"
			class="mt-1 block rounded-md px-2 py-1 text-xs text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
		>
			Все страницы
		</router-link>
		<router-link
			:to="`/${workspaceCode}/pages/_/trash`"
			class="flex items-center gap-1.5 rounded-md px-2 py-1 text-xs text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
		>
			<Trash2 class="h-3 w-3" />
			Корзина
		</router-link>

		<Dialog :open="!!renaming" @update:open="(open) => !open && (renaming = null)">
			<DialogContent class="max-w-sm">
				<DialogHeader>
					<DialogTitle>Переименовать страницу</DialogTitle>
				</DialogHeader>
				<form class="flex flex-col gap-3" @submit.prevent="confirmRename">
					<input
						v-model="renameTitle"
						data-selectable
						maxlength="255"
						class="h-9 rounded-md border border-input bg-background px-3 text-sm outline-none focus:ring-2 focus:ring-ring dark:border-input dark:bg-background"
						placeholder="Название"
					/>
					<DialogFooter>
						<button
							type="submit"
							class="h-9 rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
							:disabled="!renameTitle.trim()"
						>
							Сохранить
						</button>
					</DialogFooter>
				</form>
			</DialogContent>
		</Dialog>

		<Confirm
			v-if="deleting"
			title="Удалить страницу"
			body=""
			@on-ok="confirmDelete"
			@on-cancel="deleting = null"
		>
			<template #body>
				<p class="mt-1 text-tmgr-blue dark:text-gray-300">
					«{{ deleting.title }}» и все вложенные страницы будут перемещены в
					<router-link
						:to="`/${workspaceCode}/pages/_/trash`"
						class="underline"
						@click="deleting = null"
						>корзину</router-link
					>. Их можно восстановить оттуда.
				</p>
			</template>
		</Confirm>
	</SidebarGroup>
</template>

<script lang="ts">
	import type { PageSummary, PageType } from '@/actions/tmgr/pages';
	import Confirm from '@/components/general/Confirm.vue';
	import {
		Dialog,
		DialogContent,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import {
		DropdownMenu,
		DropdownMenuContent,
		DropdownMenuItem,
		DropdownMenuTrigger,
	} from '@/components/ui/dropdown-menu';
	import { SidebarGroup, SidebarGroupLabel } from '@/components/ui/sidebar';
	import { usePagesActions } from '@/composable/usePagesActions';
	import { usePagesTree } from '@/composable/usePagesTree';
	import {
		descendantIds,
		PAGE_CREATE_OPTIONS,
	} from '@/utils/pagesTree';
	import { ChevronDown, Plus, Trash2 } from 'lucide-vue-next';
	import {
		computed,
		defineComponent,
		provide,
		ref,
		toRef,
		watch,
	} from 'vue';
	import { useRoute, useRouter } from 'vue-router';
	import Draggable from 'vuedraggable';
	import PagesTreeNode from './PagesTreeNode.vue';
	import { PAGES_TREE_KEY, type PagesTreeContext } from './context';

	export default defineComponent({
		name: 'SidebarPagesSection',
		components: {
			ChevronDown,
			Confirm,
			Dialog,
			DialogContent,
			DialogFooter,
			DialogHeader,
			DialogTitle,
			Draggable,
			DropdownMenu,
			DropdownMenuContent,
			DropdownMenuItem,
			DropdownMenuTrigger,
			PagesTreeNode,
			Plus,
			SidebarGroup,
			SidebarGroupLabel,
			Trash2,
		},
		props: {
			workspaceId: { type: Number, required: true },
			workspaceCode: { type: String, required: true },
		},
		setup(props) {
			const route = useRoute();
			const router = useRouter();
			const workspaceId = toRef(props, 'workspaceId');
			const workspaceCode = toRef(props, 'workspaceCode');
			const actions = usePagesActions(() => props.workspaceCode);
			const tree = usePagesTree(
				computed(() => workspaceId.value),
				computed(() => true),
				() => actions.fail('Не удалось переместить страницу'),
			);

			const renaming = ref<PageSummary | null>(null);
			const renameTitle = ref('');
			const deleting = ref<PageSummary | null>(null);

			watch(
				() => [route.params.slug, tree.loaded.value],
				() => tree.revealPage(route.params.slug as string | undefined),
				{ immediate: true },
			);

			const createRoot = async (type: PageType) => {
				await actions.create(type, null);
			};

			const createChild = async (parent: PageSummary, type: PageType) => {
				tree.expand(parent.id);
				await actions.create(type, parent.id);
			};

			const requestRename = (page: PageSummary) => {
				renaming.value = page;
				renameTitle.value = page.title;
			};

			const confirmRename = async () => {
				const page = renaming.value;
				if (!page) return;
				renaming.value = null;
				if (await actions.rename(page, renameTitle.value)) await tree.refresh();
			};

			const confirmDelete = async () => {
				const page = deleting.value;
				deleting.value = null;
				if (!page) return;
				const affected = descendantIds(tree.pages.value, page.id);
				const viewing = tree.pages.value.find(
					(p) => p.slug === route.params.slug,
				);
				if (await actions.remove(page)) {
					if (viewing && affected.has(viewing.id)) {
						router.push(`/${props.workspaceCode}/pages`);
					}
					await tree.refresh();
				}
			};

			const togglePin = async (page: PageSummary) => {
				if (await actions.togglePin(page)) await tree.refresh();
			};

			const onChange: PagesTreeContext['onChange'] = (parentId, event) => {
				const change = event.added ?? event.moved;
				if (change) tree.move(change.element.id, parentId, change.newIndex);
			};

			provide(PAGES_TREE_KEY, {
				workspaceCode,
				expanded: tree.expanded,
				toggle: tree.toggle,
				createChild,
				requestRename,
				requestDelete: (page) => (deleting.value = page),
				togglePin,
				onChange,
			});

			return {
				tree: tree.tree,
				pages: tree.pages,
				loaded: tree.loaded,
				failed: tree.failed,
				renaming,
				renameTitle,
				deleting,
				createRoot,
				confirmRename,
				confirmDelete,
				onChange,
				createOptions: PAGE_CREATE_OPTIONS,
			};
		},
	});
</script>
