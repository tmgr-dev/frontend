<template>
	<div>
		<ForbiddenAccess v-if="forbidden" />

		<FeatureGate
			v-else
			feature-key="categories"
			title="Project Categories"
			description="Organize your tasks into categories and subcategories. Create a hierarchical structure for better project management."
			:icon="FolderOpen"
		>
			<template #preview>
				<CategoriesPreview />
			</template>

			<PageContainer
				:width="
					state === 'create' || (state === 'view' && tab === 'settings')
						? 'narrow'
						: 'wide'
				"
			>
				<AsyncContent
					:pending="pending"
					:loaded="loaded"
					:error="error"
					:retry="refresh"
					label="Loading categories"
				>
					<template #skeleton>
						<div class="flex flex-col gap-5 pt-2 md:pt-5">
							<Skeleton class="h-4 w-24" />
							<Skeleton class="h-10 w-56" />
							<Skeleton class="h-10 w-full" />
							<Skeleton v-for="n in 4" :key="n" class="h-12 w-full" />
						</div>
					</template>

					<div
						v-if="state === 'empty' || state === 'missing'"
						class="mx-auto mt-[72px] flex w-full max-w-[420px] flex-col items-center gap-3 text-center"
						:data-testid="`category-${state}`"
					>
						<span
							class="grid h-11 w-11 place-items-center rounded-xl border border-line bg-surface text-ink-subtle"
						>
							<component
								:is="state === 'empty' ? Folder : FolderX"
								class="h-5 w-5"
							/>
						</span>
						<template v-if="state === 'empty'">
							<h1 class="text-base font-semibold text-ink">
								No categories yet
							</h1>
							<p class="text-sm leading-normal text-ink-muted">
								A category groups tasks and sets defaults for new ones: title
								pattern like
								<code class="font-mono text-xs">TM-{index#category}</code>,
								hourly rate, estimate.
							</p>
							<Button
								data-tour="categories.create"
								class="mt-1 h-9 px-3.5 text-sm font-semibold"
								@click="goCreate(null)"
							>
								New category
							</Button>
						</template>
						<template v-else>
							<h1 class="text-base font-semibold text-ink">
								Category not found
							</h1>
							<p class="text-sm leading-normal text-ink-muted">
								It may have been deleted or moved to another workspace.
							</p>
							<Button
								v-if="workspaceCode"
								variant="outline"
								class="mt-1 h-9 px-3.5 text-sm"
								@click="goIndex"
							>
								Open categories
							</Button>
						</template>
					</div>

					<div
						v-else-if="state === 'view' || state === 'create'"
						class="flex flex-col gap-8 pt-2 md:pt-5"
					>
						<div class="flex flex-col gap-5">
							<header
								data-page-header
								class="flex flex-wrap items-end gap-x-4 gap-y-3"
							>
								<div class="min-w-0 flex-[1_1_240px]">
									<p class="mb-1 truncate text-[13px] text-ink-muted">
										{{ crumb }}
									</p>
									<h1
										class="break-words text-[28px] font-semibold leading-tight tracking-[-0.02em] text-ink md:text-[34px]"
									>
										{{ heading }}
									</h1>
								</div>
								<Button
									v-if="state === 'view'"
									type="button"
									variant="outline"
									size="sm"
									class="border-line-strong bg-surface text-[13px] text-ink"
									@click="goCreate(node.category.id)"
								>
									<Plus />
									Subcategory
								</Button>
							</header>
							<nav
								v-if="state === 'view'"
								aria-label="Category"
								class="-mx-4 flex gap-6 overflow-x-auto border-b border-line px-4 md:mx-0 md:px-0"
							>
								<router-link
									v-for="item in tabs"
									:key="item.id"
									:to="{ query: item.id === 'tasks' ? {} : { tab: item.id } }"
									:aria-current="tab === item.id ? 'page' : undefined"
									class="-mb-px inline-flex h-10 shrink-0 items-center whitespace-nowrap border-b-2 px-0.5 text-sm font-medium transition-colors"
									:class="
										tab === item.id
											? 'border-brand text-ink'
											: 'border-transparent text-ink-muted hover:text-ink'
									"
								>
									{{ item.label }}
								</router-link>
							</nav>
						</div>

						<ProjectCategoryList
							v-if="state === 'view' && tab !== 'settings'"
							:key="tab"
							:section="tab"
							@changed="refresh"
						/>
						<CategorySettingsPane
							v-else
							:key="paneKey"
							:node="node"
							:parent-node="parentNode"
							:is-create="state === 'create'"
							:nodes="nodes"
							:catalogue="catalogue"
							:catalogue-state="catalogueState"
							:workspace-name="workspaceName"
							@dirty="dirty = $event"
							@close="cancelCreate"
							@saved="refresh"
							@created="onCreated"
							@deleted="onRemoved"
							@moved="onRemoved"
							@integration-changed="refresh"
						/>
					</div>
				</AsyncContent>
			</PageContainer>
		</FeatureGate>

		<AlertDialog :open="confirm.open" @update:open="(v) => !v && decide(false)">
			<AlertDialogContent>
				<AlertDialogHeader>
					<AlertDialogTitle>Discard unsaved changes?</AlertDialogTitle>
					<AlertDialogDescription>
						Your edits to this category will be lost.
					</AlertDialogDescription>
				</AlertDialogHeader>
				<AlertDialogFooter>
					<AlertDialogCancel>Keep editing</AlertDialogCancel>
					<Button type="button" @click="decide(true)">Discard</Button>
				</AlertDialogFooter>
			</AlertDialogContent>
		</AlertDialog>
	</div>
</template>

<script>
	import { getCategorySettings } from '@/actions/tmgr/settings';
	import AsyncContent from '@/components/async/AsyncContent.vue';
	import CategorySettingsPane from '@/components/categories/CategorySettingsPane.vue';
	import ForbiddenAccess from '@/components/ForbiddenAccess.vue';
	import FeatureGate from '@/components/general/FeatureGate.vue';
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import CategoriesPreview from '@/components/previews/CategoriesPreview.vue';
	import {
		AlertDialog,
		AlertDialogCancel,
		AlertDialogContent,
		AlertDialogDescription,
		AlertDialogFooter,
		AlertDialogHeader,
		AlertDialogTitle,
	} from '@/components/ui/alert-dialog';
	import { Button } from '@/components/ui/button';
	import { Skeleton } from '@/components/ui/skeleton';
	import {
		lastCategoryKey,
		useCategoryTree,
	} from '@/composable/useCategoryTree';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import ProjectCategoryList from '@/pages/ProjectCategoryList.vue';
	import { Folder, FolderOpen, FolderX, Plus } from 'lucide-vue-next';
	import { computed, defineComponent, reactive, ref, watch } from 'vue';
	import {
		onBeforeRouteLeave,
		onBeforeRouteUpdate,
		useRoute,
		useRouter,
	} from 'vue-router';
	import { useStore } from 'vuex';

	const TABS = [
		{ id: 'tasks', label: 'Tasks' },
		{ id: 'subcategories', label: 'Subcategories' },
		{ id: 'settings', label: 'Settings' },
	];
	const INDEX_ROUTES = ['WorkspaceCategories', 'FallbackCategoriesList'];

	export default defineComponent({
		name: 'CategoryPage',
		components: {
			AlertDialog,
			AlertDialogCancel,
			AlertDialogContent,
			AlertDialogDescription,
			AlertDialogFooter,
			AlertDialogHeader,
			AlertDialogTitle,
			AsyncContent,
			Button,
			CategoriesPreview,
			CategorySettingsPane,
			FeatureGate,
			ForbiddenAccess,
			PageContainer,
			Plus,
			ProjectCategoryList,
			Skeleton,
		},
		setup() {
			const route = useRoute();
			const router = useRouter();
			const store = useStore();
			const tree = useCategoryTree();
			if (route.meta.fallback && !tree.loaded.value) void tree.refresh();
			else tree.ensure();
			const catalogue = ref(null);
			const catalogueState = ref('loading');
			const dirty = ref(false);
			const confirm = reactive({ open: false, resolve: null });

			const workspaceCode = computed(
				() =>
					route.params.workspace_code ||
					store.getters.currentWorkspace?.code ||
					null,
			);
			const workspaceName = computed(
				() => store.getters.currentWorkspace?.name || 'Workspace',
			);
			const routeName = computed(() => String(route.name));
			const findNode = (id) =>
				tree.nodes.value.find((n) => n.category.id === id) ?? null;

			const node = computed(() => {
				if (routeName.value === 'WorkspaceCategory') {
					const code = String(route.params.category_code).toLowerCase();
					return (
						tree.nodes.value.find(
							(n) => (n.category.code ?? '').toLowerCase() === code,
						) ?? null
					);
				}
				if (route.params.id) return findNode(Number(route.params.id));
				return null;
			});
			const parentNode = computed(() =>
				routeName.value === 'WorkspaceCategoryCreateInCategory'
					? findNode(Number(route.params.category_id))
					: null,
			);

			const rechecked = ref(false);
			const resolved = computed(() => {
				if (!tree.loaded.value) return 'loading';
				if (INDEX_ROUTES.includes(routeName.value))
					return tree.nodes.value.length ? 'redirect' : 'empty';
				if (routeName.value === 'WorkspaceCategoryCreate') return 'create';
				if (routeName.value === 'WorkspaceCategoryCreateInCategory')
					return parentNode.value ? 'create' : 'missing';
				if (!node.value) return 'missing';
				return routeName.value === 'WorkspaceCategory' ? 'redirect' : 'view';
			});
			const state = computed(() =>
				resolved.value === 'missing' && !rechecked.value
					? 'loading'
					: resolved.value,
			);
			watch(
				resolved,
				(value) => {
					if (value !== 'missing' || rechecked.value) return;
					void tree.refresh().finally(() => (rechecked.value = true));
				},
				{ immediate: true },
			);

			const tab = computed(() =>
				TABS.some((t) => t.id === route.query.tab) ? route.query.tab : 'tasks',
			);
			const paneKey = computed(() =>
				state.value === 'create'
					? `create:${parentNode.value?.category.id ?? 'top'}`
					: `edit:${node.value?.category.id}`,
			);

			const crumbOf = (ancestors) =>
				ancestors.length
					? ancestors.map((a) => a.title).join(' / ')
					: 'Top level';
			const crumb = computed(() => {
				if (state.value === 'create')
					return parentNode.value
						? crumbOf([
								...parentNode.value.ancestors,
								parentNode.value.category,
						  ])
						: 'Top level';
				return crumbOf(node.value?.ancestors ?? []);
			});
			const heading = computed(() =>
				state.value === 'create' ? 'New category' : node.value?.category.title,
			);

			const categoryRoute = (id, query = {}) =>
				workspaceCode.value
					? {
							name: 'WorkspaceCategoryChildren',
							params: { workspace_code: workspaceCode.value, id },
							query,
					  }
					: { name: 'ProjectCategoryChildrenList', params: { id }, query };

			const lastId = () => {
				if (!workspaceCode.value) return null;
				const id = Number(
					localStorage.getItem(lastCategoryKey(workspaceCode.value)),
				);
				return findNode(id) ? id : null;
			};

			watch(
				state,
				(value) => {
					if (value === 'redirect') {
						if (routeName.value === 'WorkspaceCategory')
							return router.replace(
								categoryRoute(node.value.category.id, route.query),
							);
						return router.replace(
							categoryRoute(lastId() ?? tree.groups.value[0].root.id),
						);
					}
					if (value === 'view' && workspaceCode.value)
						localStorage.setItem(
							lastCategoryKey(workspaceCode.value),
							String(node.value.category.id),
						);
				},
				{ immediate: true },
			);

			watch(
				() =>
					state.value === 'missing'
						? 'Category not found'
						: state.value === 'empty'
						? 'Categories'
						: state.value === 'create'
						? heading.value
						: node.value?.category.title,
				(title) => {
					if (!title) return;
					store.commit('setMetaTitle', title);
					setDocumentTitle(title);
				},
				{ immediate: true },
			);

			const loadCatalogue = async (useCache = true) => {
				catalogueState.value = 'loading';
				try {
					const data = await getCategorySettings(useCache);
					catalogue.value = Array.isArray(data) ? data : [];
					catalogueState.value = 'ready';
				} catch {
					catalogue.value = null;
					catalogueState.value = 'unavailable';
				}
			};

			const goCreate = (parentId) =>
				router.push(
					parentId
						? {
								name: 'WorkspaceCategoryCreateInCategory',
								params: {
									workspace_code: workspaceCode.value,
									category_id: String(parentId),
								},
						  }
						: {
								name: 'WorkspaceCategoryCreate',
								params: { workspace_code: workspaceCode.value },
						  },
				);
			const goIndex = () =>
				router.push({
					name: 'WorkspaceCategories',
					params: { workspace_code: workspaceCode.value },
				});
			const cancelCreate = () =>
				parentNode.value
					? router.push(categoryRoute(parentNode.value.category.id))
					: goIndex();

			const onCreated = async (id) => {
				dirty.value = false;
				await tree.refresh();
				await router.replace(categoryRoute(id, { tab: 'settings' }));
			};
			const onRemoved = async () => {
				dirty.value = false;
				const removed = node.value?.category.id;
				const ancestors = node.value?.ancestors ?? [];
				const parent = ancestors[ancestors.length - 1]?.id ?? null;
				const key = workspaceCode.value && lastCategoryKey(workspaceCode.value);
				if (key && Number(localStorage.getItem(key)) === removed)
					localStorage.removeItem(key);
				await tree.refresh();
				if (parent && findNode(parent))
					await router.replace(categoryRoute(parent));
				else
					await router.replace({
						name: 'WorkspaceCategories',
						params: { workspace_code: workspaceCode.value },
					});
			};

			const ask = () =>
				new Promise((resolve) => {
					confirm.resolve = resolve;
					confirm.open = true;
				});
			const decide = (ok) => {
				const resolve = confirm.resolve;
				confirm.open = false;
				confirm.resolve = null;
				if (ok) dirty.value = false;
				resolve?.(ok);
			};
			onBeforeRouteUpdate((to, from) => {
				if (!dirty.value) return true;
				if (
					to.path === from.path &&
					String(to.query.tab ?? '') === String(from.query.tab ?? '')
				)
					return true;
				return ask();
			});
			onBeforeRouteLeave(() => (dirty.value ? ask() : true));

			watch(
				() => store.getters.currentWorkspaceId,
				(next, prev) => {
					tree.ensure();
					if (prev !== undefined && next !== prev) void loadCatalogue(false);
				},
				{ immediate: true },
			);
			void loadCatalogue();

			return {
				Folder,
				FolderOpen,
				FolderX,
				tabs: TABS,
				tab,
				state,
				node,
				parentNode,
				nodes: tree.nodes,
				loaded: tree.loaded,
				pending: tree.pending,
				error: tree.error,
				forbidden: tree.forbidden,
				refresh: tree.refresh,
				catalogue,
				catalogueState,
				dirty,
				confirm,
				crumb,
				heading,
				paneKey,
				workspaceCode,
				workspaceName,
				goCreate,
				goIndex,
				cancelCreate,
				onCreated,
				onRemoved,
				decide,
			};
		},
	});
</script>
