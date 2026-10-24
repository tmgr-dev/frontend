<template>
	<SidebarGroup
		class="group-data-[collapsible=icon]:hidden"
		data-testid="categories-sidebar-section"
	>
		<SidebarGroupLabel>Categories</SidebarGroupLabel>
		<div class="absolute top-3.5 right-3 flex items-center gap-0.5">
			<button
				type="button"
				class="flex h-5 w-5 items-center justify-center rounded text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
				:class="
					filterOpen && 'bg-sidebar-accent text-sidebar-accent-foreground'
				"
				title="Filter categories"
				aria-label="Filter categories"
				:aria-expanded="filterOpen"
				@click="toggleFilter"
			>
				<Search class="h-3.5 w-3.5" />
			</button>
			<button
				type="button"
				class="flex h-5 w-5 items-center justify-center rounded text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
				title="New category"
				aria-label="New category"
				data-tour="categories.create"
				@click="createTop"
			>
				<Plus class="h-4 w-4" />
			</button>
		</div>

		<input
			v-if="filterOpen"
			ref="filterInput"
			v-model="filter"
			type="search"
			placeholder="Filter categories"
			aria-label="Filter categories"
			class="mb-1 h-7 w-full rounded-md border border-sidebar-border bg-transparent px-2 text-sm text-sidebar-foreground outline-hidden placeholder:text-sidebar-foreground/50 focus:border-line-focus"
			@keydown.esc="closeFilter"
		/>

		<div
			v-if="!loaded && pending"
			class="px-2 py-1 text-xs text-sidebar-foreground/60"
		>
			Loading…
		</div>
		<div
			v-else-if="!loaded && error"
			class="flex items-center gap-2 px-2 py-1 text-xs text-destructive"
		>
			Failed to load categories
			<button type="button" class="underline" @click="refresh">Retry</button>
		</div>
		<template v-else>
			<p
				v-if="filter.trim() && !rows.length"
				class="px-2 py-1 text-xs text-sidebar-foreground/60"
			>
				No categories match “{{ filter.trim() }}”
			</p>
			<ul class="flex flex-col gap-px">
				<li v-for="row in rows" :key="row.key">
					<div
						class="flex items-center rounded-md pr-1.5 text-sm text-sidebar-foreground hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
						:class="
							(row.current || row.draft) &&
							'bg-sidebar-accent font-medium text-sidebar-accent-foreground'
						"
						:style="{ paddingLeft: `${row.depth * 12}px` }"
					>
						<button
							v-if="row.hasChildren"
							type="button"
							class="flex h-7 w-6 shrink-0 items-center justify-center rounded text-sidebar-foreground/60 hover:text-sidebar-foreground"
							:aria-label="`${row.expanded ? 'Collapse' : 'Expand'} ${
								row.title
							}`"
							:aria-expanded="row.expanded"
							@click="toggle(row.id)"
						>
							<ChevronRight
								class="h-3.5 w-3.5 transition-transform"
								:class="row.expanded && 'rotate-90'"
							/>
						</button>
						<span v-else class="h-7 w-6 shrink-0" aria-hidden="true" />
						<span
							v-if="row.draft"
							aria-current="page"
							class="flex h-7 min-w-0 flex-1 items-center gap-2 italic"
						>
							<FolderPlus class="h-4 w-4 shrink-0" aria-hidden="true" />
							<span class="truncate">New category</span>
						</span>
						<router-link
							v-else
							:to="row.to"
							data-tour="categories.row"
							:aria-current="row.current ? 'page' : undefined"
							class="flex h-7 min-w-0 flex-1 items-center gap-2"
						>
							<Folder class="h-4 w-4 shrink-0" aria-hidden="true" />
							<span class="min-w-0 flex-1 truncate">{{ row.title }}</span>
							<Bot
								v-if="row.cursor"
								role="img"
								aria-label="Cursor connected"
								class="h-3.5 w-3.5 shrink-0 text-status-done"
							/>
							<TriangleAlert
								v-if="row.literal"
								role="img"
								aria-label="Title pattern has no placeholders"
								class="h-3.5 w-3.5 shrink-0 text-status-progress"
							/>
						</router-link>
					</div>
				</li>
			</ul>
		</template>
	</SidebarGroup>
</template>

<script lang="ts">
	import { SidebarGroup, SidebarGroupLabel } from '@/components/ui/sidebar';
	import { useCategoryTree } from '@/composable/useCategoryTree';
	import {
		isLiteralPattern,
		settingValue,
		TITLE_PATTERN_KEY,
	} from '@/utils/categorySettingsPage';
	import { generateCategoryUrl } from '@/utils/url';
	import {
		Bot,
		ChevronRight,
		Folder,
		FolderPlus,
		Plus,
		Search,
		TriangleAlert,
	} from 'lucide-vue-next';
	import { computed, defineComponent, nextTick, ref, watch } from 'vue';
	import { useRoute, useRouter } from 'vue-router';
	import { useStore } from 'vuex';

	export default defineComponent({
		name: 'SidebarCategoriesSection',
		components: {
			Bot,
			ChevronRight,
			Folder,
			FolderPlus,
			Plus,
			Search,
			SidebarGroup,
			SidebarGroupLabel,
			TriangleAlert,
		},
		props: {
			workspaceCode: { type: String, required: true },
		},
		setup(props) {
			const route = useRoute();
			const router = useRouter();
			const store = useStore();
			const tree = useCategoryTree();
			const filter = ref('');
			const filterOpen = ref(false);
			const filterInput = ref<HTMLInputElement | null>(null);
			const expanded = ref(new Set<number>());

			const currentId = computed(() => {
				if (route.name === 'WorkspaceCategoryChildren')
					return Number(route.params.id) || null;
				if (route.name === 'WorkspaceCategory') {
					const code = String(route.params.category_code).toLowerCase();
					return (
						tree.nodes.value.find(
							(n) => (n.category.code ?? '').toLowerCase() === code,
						)?.category.id ?? null
					);
				}
				return null;
			});
			const draftParent = computed(() => {
				if (route.name === 'WorkspaceCategoryCreate') return 0;
				if (route.name === 'WorkspaceCategoryCreateInCategory')
					return Number(route.params.category_id) || null;
				return null;
			});
			const parents = computed(
				() =>
					new Set(
						tree.nodes.value.flatMap((n) =>
							n.ancestors.length
								? [n.ancestors[n.ancestors.length - 1].id]
								: [],
						),
					),
			);

			const reveal = () => {
				const target = currentId.value || draftParent.value;
				const node = tree.nodes.value.find((n) => n.category.id === target);
				if (!node) return;
				const next = new Set(expanded.value);
				node.ancestors.forEach((a) => next.add(a.id));
				if (draftParent.value) next.add(node.category.id);
				expanded.value = next;
			};
			watch([currentId, draftParent, () => tree.nodes.value], reveal, {
				immediate: true,
			});

			const toggle = (id: number) => {
				const next = new Set(expanded.value);
				if (next.has(id)) next.delete(id);
				else next.add(id);
				expanded.value = next;
			};

			const rows = computed(() => {
				const query = filter.value.trim().toLowerCase();
				let nodes = tree.nodes.value;
				if (query) {
					const keep = new Set<number>();
					for (const node of nodes) {
						const text = `${node.category.title} ${node.category.code ?? ''}`;
						if (!text.toLowerCase().includes(query)) continue;
						keep.add(node.category.id);
						node.ancestors.forEach((a) => keep.add(a.id));
					}
					nodes = nodes.filter((n) => keep.has(n.category.id));
				} else {
					nodes = nodes.filter((n) =>
						n.ancestors.every((a) => expanded.value.has(a.id)),
					);
				}
				const list: any[] = nodes.map((node) => {
					const id = node.category.id;
					return {
						key: id,
						id,
						depth: node.depth,
						title: node.category.title,
						to: generateCategoryUrl(id, { code: props.workspaceCode }),
						current: id === currentId.value,
						cursor: !!node.category.cursor_connected,
						literal: isLiteralPattern(
							settingValue(node.category, TITLE_PATTERN_KEY),
						),
						hasChildren: parents.value.has(id),
						expanded: !!query || expanded.value.has(id),
					};
				});
				if (draftParent.value !== null && !query) {
					const draft = { key: 'draft', draft: true, depth: 0 };
					const at = list.findIndex((r) => r.id === draftParent.value);
					if (at < 0) list.push(draft);
					else list.splice(at + 1, 0, { ...draft, depth: list[at].depth + 1 });
				}
				return list;
			});

			const toggleFilter = async () => {
				filterOpen.value = !filterOpen.value;
				if (!filterOpen.value) {
					filter.value = '';
					return;
				}
				await nextTick();
				filterInput.value?.focus();
			};
			const closeFilter = () => {
				filter.value = '';
				filterOpen.value = false;
			};

			const createTop = () =>
				router.push({
					name: 'WorkspaceCategoryCreate',
					params: { workspace_code: props.workspaceCode },
				});

			watch(() => store.getters.currentWorkspaceId, tree.ensure, {
				immediate: true,
			});

			return {
				filter,
				filterOpen,
				filterInput,
				rows,
				loaded: tree.loaded,
				pending: tree.pending,
				error: tree.error,
				refresh: tree.refresh,
				toggle,
				toggleFilter,
				closeFilter,
				createTop,
			};
		},
	});
</script>
