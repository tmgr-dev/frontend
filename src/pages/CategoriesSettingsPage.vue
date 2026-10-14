<template>
	<div>
		<ForbiddenAccess v-if="permissionDenied" />

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

			<PageContainer width="wide">
				<header
					data-page-header
					class="mb-3 flex items-center justify-between gap-3 lg:mb-4 lg:items-start"
					:class="phonePane && !isCreate && 'max-lg:hidden'"
				>
					<div class="flex min-w-0 flex-col gap-1">
						<h1
							class="text-[22px] font-semibold leading-tight text-ink lg:text-2xl"
						>
							Categories
						</h1>
						<p class="hidden text-sm text-ink-subtle lg:block">
							{{ subtitle }}
						</p>
					</div>
					<Button
						v-if="!isEmpty"
						data-tour="categories.create"
						aria-label="New category"
						class="h-9 shrink-0 gap-1.5 px-3.5 text-sm font-semibold max-lg:w-9 max-lg:rounded-full max-lg:p-0"
						@click="startCreate(null)"
					>
						<Plus class="max-lg:!size-[18px]" />
						<span class="max-lg:sr-only">New category</span>
					</Button>
				</header>

				<AsyncContent
					:pending="pending"
					:loaded="loaded"
					:error="error"
					:retry="() => load(true)"
					label="Loading categories"
				>
					<template #skeleton>
						<div
							class="flex h-[calc(100dvh-12.5rem)] min-h-[28rem] overflow-hidden rounded-xl border border-line bg-surface"
						>
							<div
								class="w-full space-y-3 p-3 lg:w-[300px] lg:border-r lg:border-line"
							>
								<Skeleton v-for="n in 6" :key="n" class="h-9 w-full" />
							</div>
						</div>
					</template>

					<div
						v-if="isEmpty"
						class="mx-auto mt-[72px] flex w-full max-w-[420px] flex-col items-center gap-3 text-center"
					>
						<span
							class="grid h-11 w-11 place-items-center rounded-xl border border-line bg-surface text-ink-subtle"
						>
							<Folder class="h-5 w-5" />
						</span>
						<h2 class="text-base font-semibold text-ink">No categories yet</h2>
						<p class="text-sm leading-normal text-ink-muted">
							A category groups tasks and sets defaults for new ones: title
							pattern like
							<code class="font-mono text-xs">TM-{index#category}</code>, hourly
							rate, estimate.
						</p>
						<Button
							data-tour="categories.create"
							class="mt-1 h-9 px-3.5 text-sm font-semibold"
							@click="startCreate(null)"
						>
							New category
						</Button>
					</div>

					<div
						v-else
						class="flex min-h-0 lg:h-[calc(100dvh-12.5rem)] lg:min-h-[28rem] lg:overflow-hidden lg:rounded-xl lg:border lg:border-line lg:bg-surface"
					>
						<aside
							aria-label="Categories"
							class="flex min-h-0 w-full flex-col lg:w-[300px] lg:flex-none lg:border-r lg:border-line"
							:class="phonePane && 'max-lg:hidden'"
						>
							<div class="pb-3 lg:border-b lg:border-line lg:p-3">
								<label
									class="flex h-9 items-center gap-2 rounded-lg border border-line-strong bg-surface px-2.5 text-ink-subtle focus-within:border-line-focus lg:h-8 lg:bg-surface-base"
								>
									<Search class="h-3.5 w-3.5 flex-none" />
									<input
										ref="filterInput"
										v-model="filter"
										type="search"
										placeholder="Filter categories"
										aria-label="Filter categories"
										class="min-w-0 flex-1 border-0 bg-transparent text-sm text-ink outline-none placeholder:text-ink-subtle lg:text-[13px]"
										@keydown.esc="filter = ''"
									/>
									<kbd
										class="hidden rounded border border-line-strong px-1.5 font-mono text-[11px] lg:inline"
										>/</kbd
									>
								</label>
							</div>
							<div
								role="listbox"
								aria-label="Categories"
								class="flex min-h-0 flex-1 flex-col gap-3 lg:gap-0 lg:overflow-y-auto lg:px-2 lg:pb-3 lg:pt-1.5"
							>
								<p
									v-if="!visibleGroups.length && filter.trim()"
									class="px-2.5 py-3 text-sm text-ink-subtle"
								>
									No categories match “{{ filter }}”.
								</p>
								<div
									v-for="group in visibleGroups"
									:key="group.root.id"
									role="group"
									:aria-label="group.root.title"
									class="overflow-hidden rounded-xl border border-line bg-surface lg:overflow-visible lg:rounded-none lg:border-0 lg:bg-transparent"
									:class="nested && 'lg:mt-2.5 lg:first:mt-0'"
								>
									<template v-for="node in group.nodes" :key="node.category.id">
										<div
											role="option"
											data-tour="categories.row"
											tabindex="0"
											:aria-selected="selectedId === node.category.id"
											:style="{
												'--indent': `${node.depth * 14}px`,
												'--pad': `${12 + node.depth * 16}px`,
											}"
											class="flex min-h-[52px] cursor-pointer items-center gap-2 border-t border-line py-1.5 pl-[var(--pad)] pr-3 outline-none first:border-t-0 hover:bg-surface-hover focus-visible:bg-surface-hover lg:ml-[var(--indent)] lg:min-h-9 lg:rounded-lg lg:border-0 lg:px-2.5 lg:py-[5px]"
											:class="
												selectedId === node.category.id &&
												!isCreate &&
												'bg-surface-active shadow-[inset_2px_0_0_hsl(var(--primary))] hover:bg-surface-active'
											"
											@click="select(node.category.id)"
											@keydown.enter.prevent="select(node.category.id)"
											@keydown.space.prevent="select(node.category.id)"
										>
											<span class="flex min-w-0 flex-1 flex-col gap-px">
												<span
													class="truncate text-[15px] text-ink"
													:class="
														nested && node.depth === 0
															? 'font-semibold lg:text-[13px]'
															: 'lg:text-sm'
													"
												>
													{{ node.category.title }}
												</span>
												<span
													v-if="node.meta"
													class="truncate font-mono text-[11px]"
													:class="
														node.literal
															? 'text-status-progress'
															: 'text-ink-subtle'
													"
												>
													{{ node.meta }}
												</span>
											</span>
											<span
												v-if="node.category.cursor_connected"
												title="Cursor connected"
												class="h-[7px] w-[7px] flex-none rounded-full bg-status-done max-lg:hidden"
											/>
											<TriangleAlert
												v-if="node.literal"
												aria-label="Title pattern has no placeholders"
												class="h-3.5 w-3.5 flex-none text-status-progress max-lg:hidden"
											/>
											<ChevronRight
												class="h-4 w-4 flex-none text-ink-faint lg:hidden"
											/>
										</div>
										<div
											v-if="isCreate && createParentId === node.category.id"
											role="option"
											aria-selected="true"
											:style="{ '--indent': `${(node.depth + 1) * 14}px` }"
											class="flex h-9 items-center rounded-lg bg-surface-active px-2.5 text-sm italic text-ink-muted shadow-[inset_2px_0_0_hsl(var(--primary))] lg:ml-[var(--indent)]"
										>
											New subcategory
										</div>
									</template>
								</div>
							</div>
						</aside>

						<CategorySettingsPane
							v-if="hasPanel"
							:key="paneKey"
							:node="selectedNode"
							:parent-node="parentNode"
							:is-create="isCreate"
							:nodes="nodes"
							:catalogue="catalogue"
							:catalogue-state="catalogueState"
							:workspace-name="workspaceName"
							:tasks-link="tasksLink"
							:class="!phonePane && 'max-lg:hidden'"
							@dirty="dirty = $event"
							@close="clearSelection"
							@add-sub="startCreate(selectedNode.category.id)"
							@saved="load(true)"
							@created="onCreated"
							@deleted="onRemoved"
							@moved="onRemoved"
							@integration-changed="load(true)"
						/>

						<section
							v-else
							aria-label="Nothing selected"
							class="min-h-0 flex-1 flex-col items-center gap-7 overflow-y-auto px-12 py-14 max-lg:hidden lg:flex"
						>
							<div
								class="flex max-w-[380px] flex-col items-center gap-2.5 text-center"
							>
								<span
									class="grid h-11 w-11 place-items-center rounded-xl border border-line bg-surface-base text-ink-subtle"
								>
									<SlidersHorizontal class="h-5 w-5" />
								</span>
								<h2 class="text-base font-semibold text-ink">
									Select a category
								</h2>
								<p class="text-sm leading-normal text-ink-muted">
									Its name, parent and defaults for new tasks open here.
								</p>
							</div>

							<div
								v-if="issues.length"
								class="w-full max-w-[620px] overflow-hidden rounded-xl border border-line"
							>
								<div
									class="flex items-start gap-3 bg-status-progress-bg px-4 py-3.5 dark:bg-[oklch(0.3_0.05_75/0.35)]"
								>
									<TriangleAlert
										class="mt-0.5 h-4 w-4 flex-none text-status-progress"
									/>
									<span class="flex flex-1 flex-col gap-[3px]">
										<b class="text-sm font-semibold text-ink">
											{{ issueTitle }}
										</b>
										<span class="text-xs leading-normal text-ink-muted">
											The title pattern has no placeholders, so new tasks get
											the literal text instead of today’s date.
										</span>
									</span>
									<Button
										v-if="catalogueState === 'ready'"
										type="button"
										variant="outline"
										class="h-8 flex-none border-line-strong bg-surface px-3 text-[13px] text-ink"
										:disabled="fixing"
										@click="fixAll"
									>
										Fix all {{ issues.length }}
									</Button>
								</div>
								<button
									v-for="issue in issues"
									:key="issue.category.id"
									type="button"
									class="grid h-11 w-full grid-cols-[minmax(0,1fr)_150px_56px] items-center gap-3 border-t border-line px-4 text-left hover:bg-surface-hover"
									@click="select(issue.category.id)"
								>
									<span class="flex min-w-0 items-baseline gap-2">
										<span class="whitespace-nowrap text-sm text-ink">
											{{ issue.category.title }}
										</span>
										<span class="truncate text-xs text-ink-subtle">
											{{ issue.path }}
										</span>
									</span>
									<span class="truncate font-mono text-xs text-status-progress">
										{{ issue.pattern }}
									</span>
									<span class="text-right text-xs text-ink-muted">Open →</span>
								</button>
							</div>
						</section>
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
	import { getCategories } from '@/actions/tmgr/categories';
	import {
		getCategorySettings,
		updateCategorySettings,
	} from '@/actions/tmgr/settings';
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
	import { useToast } from '@/components/ui/toast';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import {
		buildCategoryGroups,
		categoryMeta,
		fixedSettingsPayload,
		flattenGroups,
		isLiteralPattern,
		literalIssues,
		settingValue,
		TITLE_PATTERN_KEY,
	} from '@/utils/categorySettingsPage';
	import { requestCache } from '@/utils/requestCache';
	import {
		ChevronRight,
		Folder,
		FolderOpen,
		Plus,
		Search,
		SlidersHorizontal,
		TriangleAlert,
	} from 'lucide-vue-next';
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		reactive,
		ref,
		watch,
	} from 'vue';
	import {
		onBeforeRouteLeave,
		onBeforeRouteUpdate,
		useRoute,
		useRouter,
	} from 'vue-router';
	import { useStore } from 'vuex';

	export default defineComponent({
		name: 'CategoriesSettingsPage',
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
			ChevronRight,
			FeatureGate,
			Folder,
			ForbiddenAccess,
			PageContainer,
			Plus,
			Search,
			Skeleton,
			SlidersHorizontal,
			TriangleAlert,
		},
		setup() {
			const route = useRoute();
			const router = useRouter();
			const store = useStore();
			const { toast } = useToast();

			const categories = ref([]);
			const loaded = ref(false);
			const pending = ref(true);
			const error = ref(null);
			const permissionDenied = ref(false);
			const catalogue = ref(null);
			const catalogueState = ref('loading');
			const filter = ref('');
			const filterInput = ref(null);
			const dirty = ref(false);
			const fixing = ref(false);
			const confirm = reactive({ open: false, resolve: null });
			let request = 0;

			setDocumentTitle('Categories');

			const load = async (fresh = false) => {
				const id = ++request;
				pending.value = true;
				error.value = null;
				try {
					if (fresh) requestCache.invalidate('categories');
					const data = await getCategories();
					if (id !== request) return;
					categories.value = Array.isArray(data) ? data : [];
					loaded.value = true;
				} catch (e) {
					if (id !== request) return;
					if (e?.response?.status === 403) permissionDenied.value = true;
					error.value =
						e?.response?.data?.message || 'Could not load categories.';
				} finally {
					if (id === request) pending.value = false;
				}
			};

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

			const groups = computed(() =>
				buildCategoryGroups(categories.value).map((group) => ({
					root: group.root,
					nodes: group.nodes.map((node) => ({
						...node,
						meta: categoryMeta(node.category),
						literal: isLiteralPattern(
							settingValue(node.category, TITLE_PATTERN_KEY),
						),
					})),
				})),
			);
			const nodes = computed(() => flattenGroups(groups.value));
			const nested = computed(() => nodes.value.some((n) => n.depth > 0));
			const subtitle = computed(() => {
				const total = nodes.value.length;
				if (!loaded.value || !total) return 'Group tasks into projects';
				const count = `${total} ${total === 1 ? 'category' : 'categories'}`;
				return nested.value
					? `${count} · ${groups.value.length} at the top level`
					: count;
			});

			const visibleGroups = computed(() => {
				const query = filter.value.trim().toLowerCase();
				if (!query) return groups.value;
				return groups.value
					.map((group) => {
						const keep = new Set();
						for (const node of group.nodes) {
							const text = `${node.category.title} ${node.category.code ?? ''}`;
							if (!text.toLowerCase().includes(query)) continue;
							keep.add(node.category.id);
							node.ancestors.forEach((a) => keep.add(a.id));
						}
						return {
							...group,
							nodes: group.nodes.filter((n) => keep.has(n.category.id)),
						};
					})
					.filter((group) => group.nodes.length);
			});

			const selectedId = computed(() => Number(route.query.category) || null);
			const isCreate = computed(
				() => route.query.create != null && route.query.create !== '',
			);
			const findNode = (id) =>
				nodes.value.find((n) => n.category.id === id) ?? null;
			const parentNode = computed(() =>
				isCreate.value && route.query.create !== 'top'
					? findNode(Number(route.query.create))
					: null,
			);
			const createParentId = computed(
				() => parentNode.value?.category.id ?? null,
			);
			const selectedNode = computed(() =>
				isCreate.value ? null : findNode(selectedId.value),
			);
			const hasPanel = computed(
				() => loaded.value && (isCreate.value || !!selectedNode.value),
			);
			const paneKey = computed(() =>
				isCreate.value
					? `create:${createParentId.value ?? 'top'}`
					: `edit:${selectedId.value}`,
			);
			const isEmpty = computed(
				() => loaded.value && !nodes.value.length && !isCreate.value,
			);

			const workspace = computed(() => store.getters.currentWorkspace);
			const workspaceName = computed(
				() => workspace.value?.name || 'Workspace',
			);
			const workspaceCode = computed(
				() => route.params.workspace_code || workspace.value?.code || null,
			);
			const tasksLink = computed(() => {
				const id = selectedNode.value?.category.id;
				if (!id) return null;
				return workspaceCode.value
					? {
							name: 'WorkspaceCategoryChildren',
							params: { workspace_code: workspaceCode.value, id },
					  }
					: { name: 'ProjectCategoryChildrenList', params: { id } };
			});

			const issues = computed(() => literalIssues(nodes.value));
			const issueTitle = computed(() =>
				issues.value.length === 1
					? '1 category names new tasks literally'
					: `${issues.value.length} categories name new tasks literally`,
			);

			const createdId = ref(null);
			const phonePane = computed(
				() =>
					hasPanel.value &&
					(isCreate.value || createdId.value !== selectedId.value),
			);
			watch(selectedId, (id) => {
				if (id !== createdId.value) createdId.value = null;
			});
			const select = (id) => {
				createdId.value = null;
				return router.push({ query: { category: String(id) } });
			};
			const startCreate = (parentId) =>
				router.push({ query: { create: parentId ? String(parentId) : 'top' } });
			const clearSelection = () => router.push({ query: {} });

			const onCreated = async (id) => {
				dirty.value = false;
				await load(true);
				await router.replace({ query: { category: String(id) } });
				createdId.value = id;
			};
			const onRemoved = async () => {
				dirty.value = false;
				await router.replace({ query: {} });
				await load(true);
			};

			const fixAll = async () => {
				const targets = issues.value.map((i) => i.category);
				fixing.value = true;
				let fixed = 0;
				for (const category of targets) {
					try {
						await updateCategorySettings(
							category.id,
							fixedSettingsPayload(category),
						);
						fixed += 1;
					} catch (e) {
						console.error(e);
					}
				}
				fixing.value = false;
				await load(true);
				if (fixed === targets.length) {
					toast({
						title: `Fixed ${fixed} title ${
							fixed === 1 ? 'pattern' : 'patterns'
						}`,
					});
				} else {
					toast({
						title: `Fixed ${fixed} of ${targets.length} title patterns`,
						description: `${
							targets.length - fixed
						} could not be saved. Try again for the rest.`,
						variant: 'destructive',
					});
				}
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
			const guard = (to, from) => {
				if (!dirty.value) return true;
				if (
					to.path === from.path &&
					String(to.query.category ?? '') ===
						String(from.query.category ?? '') &&
					String(to.query.create ?? '') === String(from.query.create ?? '')
				)
					return true;
				return ask();
			};
			onBeforeRouteUpdate(guard);
			onBeforeRouteLeave((to, from) => (dirty.value ? ask() : true));

			const onKeydown = (event) => {
				if (event.key !== '/' || event.metaKey || event.ctrlKey || event.altKey)
					return;
				const target = event.target;
				if (
					target?.isContentEditable ||
					['INPUT', 'TEXTAREA', 'SELECT'].includes(target?.tagName)
				)
					return;
				if (!filterInput.value) return;
				event.preventDefault();
				filterInput.value.focus();
			};

			watch(
				() => store.getters.currentWorkspaceId,
				(next, prev) => {
					if (next === prev) return;
					loaded.value = false;
					categories.value = [];
					permissionDenied.value = false;
					void load(true);
					void loadCatalogue(false);
				},
			);

			onMounted(() => {
				window.addEventListener('keydown', onKeydown);
				void load(true);
				void loadCatalogue();
			});
			onBeforeUnmount(() => {
				++request;
				window.removeEventListener('keydown', onKeydown);
			});

			return {
				FolderOpen,
				permissionDenied,
				loaded,
				pending,
				error,
				load,
				catalogue,
				catalogueState,
				filter,
				filterInput,
				dirty,
				fixing,
				confirm,
				nested,
				nodes,
				subtitle,
				visibleGroups,
				selectedId,
				isCreate,
				parentNode,
				createParentId,
				selectedNode,
				hasPanel,
				phonePane,
				paneKey,
				isEmpty,
				workspaceName,
				tasksLink,
				issues,
				issueTitle,
				select,
				startCreate,
				clearSelection,
				onCreated,
				onRemoved,
				fixAll,
				decide,
			};
		},
	});
</script>
