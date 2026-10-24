<template>
	<Dialog v-model:open="open">
		<DialogContent
			class="max-w-xl gap-0 overflow-hidden p-0 [&>button.absolute]:hidden"
			@close-auto-focus="restoreFocus"
		>
			<DialogTitle class="sr-only">Search</DialogTitle>
			<DialogDescription class="sr-only">
				Find tasks, pages, places and workspaces
			</DialogDescription>
			<Command
				v-model:search-term="query"
				v-model:selected-value="highlighted"
				data-testid="command-palette"
				:filter-function="inRenderedOrder"
				:reset-search-term-on-blur="false"
				class="dark:bg-popover"
				@keydown.capture="trackKeys"
			>
				<CommandInput
					data-testid="command-palette-input"
					placeholder="Ask or search anything…"
					class="h-12 text-base sm:text-sm"
				/>
				<CommandList class="h-[min(60vh,420px)] max-h-none p-1">
					<p
						v-if="loading"
						data-testid="command-palette-status"
						class="px-3 py-2 text-sm text-muted-foreground dark:text-muted-foreground"
					>
						Searching…
					</p>
					<p
						v-else-if="error"
						data-testid="command-palette-status"
						class="px-3 py-2 text-sm text-destructive dark:text-destructive"
					>
						Search failed
					</p>
					<p
						v-else-if="nothingFound"
						data-testid="command-palette-empty"
						class="px-3 py-6 text-center text-sm text-muted-foreground dark:text-muted-foreground"
					>
						Nothing found
					</p>
					<CommandGroup
						v-for="group in groups"
						:key="group.key"
						:heading="group.heading"
						:data-testid="`command-palette-group-${group.key}`"
					>
						<CommandItem
							v-for="item in group.items"
							:key="item.value"
							:value="item.value"
							data-testid="command-palette-item"
							:data-group="group.key"
							:data-kind="item.kind"
							class="gap-2"
							@select="(event) => choose(event, item)"
						>
							<component
								:is="item.icon"
								class="h-4 w-4 shrink-0 text-muted-foreground dark:text-muted-foreground"
							/>
							<span class="min-w-0 flex-1">
								<span class="block truncate">{{ item.label }}</span>
								<span
									v-if="item.detail"
									class="block truncate text-xs text-muted-foreground dark:text-muted-foreground"
									>{{ item.detail }}</span
								>
							</span>
							<span
								v-if="item.badge"
								class="shrink-0 rounded bg-muted px-1.5 py-0.5 text-2xs text-muted-foreground dark:bg-muted dark:text-muted-foreground"
								>{{ item.badge }}</span
							>
						</CommandItem>
					</CommandGroup>
				</CommandList>
			</Command>
		</DialogContent>
	</Dialog>
</template>

<script lang="ts">
	import { searchPages, type PageSearchHit } from '@/actions/tmgr/pages';
	import { getTasks, type Task } from '@/actions/tmgr/tasks';
	import {
		Command,
		CommandGroup,
		CommandInput,
		CommandItem,
		CommandList,
	} from '@/components/ui/command';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { useFeatureToggles } from '@/composable/useFeatureToggles';
	import {
		commandPaletteOpen,
		isCommandPaletteHotkey,
		loadRecent,
		navTargets,
		openCommandPalette,
		pushRecent,
		rankByLabel,
		resolvePaletteWorkspace,
		saveRecent,
		setCommandPaletteAvailability,
		takeReturnFocus,
		type RecentEntry,
	} from '@/utils/commandPalette';
	import { desktopPlatform, isDesktopApp } from '@/utils/desktop';
	import { openPageWindow, pageWindowTarget } from '@/utils/pageWindow';
	import {
		canSearch,
		cleanSnippet,
		normalizeQuery,
		pageHitUrl,
		pageTypeLabel,
		SEARCH_DEBOUNCE_MS,
	} from '@/utils/pagesSearch';
	import { pagesAvailable } from '@/utils/pagesTree';
	import { openTaskWindow, taskWindowTarget } from '@/utils/taskWindow';
	import { generateTaskUrl } from '@/utils/url';
	import {
		Briefcase,
		ClipboardList,
		Compass,
		FileText,
		History,
	} from 'lucide-vue-next';
	import type { ComboboxItemEmits } from 'radix-vue';
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		ref,
		watch,
		type Component,
	} from 'vue';
	import { useRoute, useRouter } from 'vue-router';
	import { useStore } from 'vuex';

	interface Workspace {
		id: number | string;
		code: string;
		name: string;
		is_local?: boolean;
	}

	type ItemKind = 'task' | 'page' | 'nav' | 'workspace';

	interface PaletteItem {
		value: string;
		kind: ItemKind;
		label: string;
		icon: Component;
		detail?: string;
		badge?: string;
		to?: string;
		recent?: RecentEntry;
		workspace?: Workspace;
	}

	type SelectEvent = ComboboxItemEmits['select'][0];

	export default defineComponent({
		name: 'CommandPalette',
		components: {
			Command,
			CommandGroup,
			CommandInput,
			CommandItem,
			CommandList,
			Dialog,
			DialogContent,
			DialogDescription,
			DialogTitle,
		},
		emits: ['switch-workspace'],
		setup(_, { emit }) {
			const store = useStore();
			const router = useRouter();
			const route = useRoute();
			const { isFeatureEnabled, showSurface } = useFeatureToggles();
			const isMac = desktopPlatform() === 'macos';
			const isDesktop = isDesktopApp();

			const open = commandPaletteOpen;
			const query = ref('');
			const highlighted = ref<string | undefined>();
			const loading = ref(false);
			const error = ref(false);
			const taskHits = ref<Task[]>([]);
			const pageHits = ref<PageSearchHit[]>([]);
			const recent = ref<RecentEntry[]>([]);
			let timer: ReturnType<typeof setTimeout> | null = null;
			let request = 0;
			let enterWithModifier = false;
			let userMoved = false;

			const workspace = computed(() =>
				resolvePaletteWorkspace<Workspace>(
					store.state.workspaces,
					route.params.workspace_code,
					store.getters.currentWorkspace,
				),
			);
			const pagesEnabled = computed(() =>
				pagesAvailable(workspace.value, isFeatureEnabled('pages')),
			);
			const trimmed = computed(() => normalizeQuery(query.value));

			const navItems = computed<PaletteItem[]>(() => {
				if (!workspace.value) return [];
				const targets = navTargets(workspace.value.code, {
					dashboard: isFeatureEnabled('dashboard'),
					board: showSurface('board.nav'),
					routines: isFeatureEnabled('daily_routines'),
					localRoutines: !!workspace.value.is_local,
					categories: isFeatureEnabled('categories'),
					pages: pagesEnabled.value,
					files: showSurface('files.nav'),
					map: isFeatureEnabled('graph'),
					aiUsage: isDesktop,
				});
				return rankByLabel(targets, trimmed.value, (t) => t.label).map(
					(target) => ({
						value: `nav:${target.key}`,
						kind: 'nav' as const,
						label: target.label,
						icon: Compass,
						to: target.to,
					}),
				);
			});

			const workspaceItems = computed<PaletteItem[]>(() => {
				const others = ((store.state.workspaces ?? []) as Workspace[]).filter(
					(w) => String(w.id) !== String(workspace.value?.id),
				);
				return rankByLabel(others, trimmed.value, (w) => w.name).map((w) => ({
					value: `workspace:${w.id}`,
					kind: 'workspace' as const,
					label: w.name,
					detail: w.code,
					badge: w.is_local ? 'Local' : undefined,
					icon: Briefcase,
					workspace: w,
				}));
			});

			const taskUrl = (task: Task) =>
				generateTaskUrl(
					task.id as number,
					workspace.value,
					typeof task.category === 'object' ? task.category : null,
				);

			const groups = computed(() => {
				const list: { key: string; heading: string; items: PaletteItem[] }[] =
					[];
				if (!trimmed.value) {
					list.push({
						key: 'recent',
						heading: 'Recent',
						items: recent.value.map((entry) => ({
							value: `recent:${entry.kind}:${entry.id}`,
							kind: entry.kind,
							label: entry.title,
							icon: entry.kind === 'page' ? FileText : History,
							recent: entry,
						})),
					});
				} else {
					list.push({
						key: 'tasks',
						heading: 'Tasks',
						items: taskHits.value.map((task) => ({
							value: `task:${task.id}`,
							kind: 'task' as const,
							label: task.title,
							icon: ClipboardList,
							recent: {
								kind: 'task' as const,
								id: Number(task.id),
								title: task.title,
								url: taskUrl(task),
							},
						})),
					});
					if (pagesEnabled.value) {
						list.push({
							key: 'pages',
							heading: 'Pages',
							items: pageHits.value.map((hit) => ({
								value: `page:${hit.id}`,
								kind: 'page' as const,
								label: hit.title,
								detail: hit.snippet ? cleanSnippet(hit.snippet) : undefined,
								badge: pageTypeLabel(hit.type),
								icon: FileText,
								recent: {
									kind: 'page' as const,
									id: hit.id,
									title: hit.title,
									url: pageHitUrl(workspace.value?.code ?? '', hit),
									slug: hit.slug,
								},
							})),
						});
					}
				}
				list.push({
					key: 'navigation',
					heading: 'Navigation',
					items: navItems.value,
				});
				list.push({
					key: 'workspaces',
					heading: 'Workspaces',
					items: workspaceItems.value,
				});
				return list.filter((group) => group.items.length);
			});

			const nothingFound = computed(
				() =>
					!!trimmed.value &&
					!loading.value &&
					!error.value &&
					groups.value.length === 0,
			);

			const run = () => {
				const current = ++request;
				const q = trimmed.value;
				let pending = 2;
				const settle = (apply: () => Promise<void>) =>
					apply()
						.catch(() => {
							if (current === request) error.value = true;
						})
						.finally(() => {
							if (current === request && --pending === 0) loading.value = false;
						});
				void settle(async () => {
					const response = await getTasks({
						page: 1,
						per_page: 10,
						params: { search: q },
					});
					if (current === request) taskHits.value = response.data;
				});
				void settle(async () => {
					if (!pagesEnabled.value) return;
					const hits = await searchPages(q, { limit: 10 });
					if (current === request) pageHits.value = hits;
				});
			};

			const schedule = () => {
				if (timer) clearTimeout(timer);
				++request;
				taskHits.value = [];
				pageHits.value = [];
				error.value = false;
				if (!canSearch(query.value)) {
					loading.value = false;
					return;
				}
				loading.value = true;
				timer = setTimeout(run, SEARCH_DEBOUNCE_MS);
			};
			watch(query, schedule);

			watch(open, (isOpen) => {
				if (isOpen) {
					const userId = store.state.user?.id;
					recent.value =
						userId && workspace.value
							? loadRecent(localStorage, userId, workspace.value.id)
							: [];
				} else {
					query.value = '';
					highlighted.value = undefined;
				}
			});

			const itemValues = computed(() =>
				groups.value.flatMap((group) => group.items.map((item) => item.value)),
			);
			// Radix re-reads item order only when the item count changes, so ranked
			// results of the same size would keep a stale arrow-key order.
			const inRenderedOrder = <T,>(_: T): T => itemValues.value as unknown as T;

			watch(trimmed, () => {
				userMoved = false;
			});
			watch(
				itemValues,
				(values) => {
					if (!userMoved || !values.includes(highlighted.value ?? ''))
						highlighted.value = values[0];
				},
				{ flush: 'post' },
			);

			const trackKeys = (event: KeyboardEvent) => {
				if (event.key === 'ArrowDown' || event.key === 'ArrowUp')
					userMoved = true;
				enterWithModifier =
					event.key === 'Enter' && (isMac ? event.metaKey : event.ctrlKey);
			};

			const remember = (entry: RecentEntry) => {
				const userId = store.state.user?.id;
				if (!userId || !workspace.value) return;
				const id = workspace.value.id;
				saveRecent(
					localStorage,
					userId,
					id,
					pushRecent(loadRecent(localStorage, userId, id), entry),
				);
			};

			const openDetached = async (entry: RecentEntry): Promise<boolean> => {
				try {
					if (entry.kind === 'task') {
						const target = taskWindowTarget(
							{ id: entry.id, title: entry.title },
							store.state.workspaces,
							workspace.value,
						);
						if (!target) return false;
						await openTaskWindow(target);
						return true;
					}
					const target = pageWindowTarget(
						{ id: entry.id, slug: entry.slug, title: entry.title },
						workspace.value?.code,
					);
					if (!target) return false;
					await openPageWindow(target);
					return true;
				} catch {
					return false;
				}
			};

			const activate = async (item: PaletteItem, newTab: boolean) => {
				open.value = false;
				if (item.workspace) {
					emit('switch-workspace', item.workspace);
					return;
				}
				if (item.recent) remember(item.recent);
				const to = item.recent?.url ?? item.to;
				if (!to) return;
				if (newTab && !isDesktop) {
					window.open(router.resolve(to).href, '_blank');
					return;
				}
				if (newTab && item.recent && (await openDetached(item.recent))) return;
				await router.push(to);
			};

			const choose = (event: SelectEvent, item: PaletteItem) => {
				event.preventDefault();
				const original = event.detail.originalEvent as MouseEvent;
				const modified = original.isTrusted
					? isMac
						? original.metaKey
						: original.ctrlKey
					: enterWithModifier;
				enterWithModifier = false;
				void activate(item, modified);
			};

			const restoreFocus = (event: Event) => {
				event.preventDefault();
				takeReturnFocus()?.focus();
			};

			const onKeydown = (event: KeyboardEvent) => {
				if (!isCommandPaletteHotkey(event, isMac)) return;
				if (!open.value && !workspace.value) return;
				event.preventDefault();
				if (open.value) open.value = false;
				else openCommandPalette();
			};

			onMounted(() => {
				setCommandPaletteAvailability(() => !!workspace.value);
				window.addEventListener('keydown', onKeydown);
			});
			onBeforeUnmount(() => {
				setCommandPaletteAvailability(() => false);
				window.removeEventListener('keydown', onKeydown);
				if (timer) clearTimeout(timer);
				open.value = false;
			});

			return {
				open,
				query,
				highlighted,
				loading,
				error,
				groups,
				nothingFound,
				inRenderedOrder,
				trackKeys,
				choose,
				restoreFocus,
			};
		},
	});
</script>
