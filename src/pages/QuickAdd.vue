<template>
	<div
		class="flex h-screen flex-col overflow-hidden border border-border bg-background text-foreground dark:border-border dark:bg-background"
		@keydown.esc.prevent="hide"
	>
		<input
			ref="fileInput"
			type="file"
			accept="image/*"
			class="hidden"
			data-testid="quick-add-file"
			@change="chooseImage"
		/>
		<div
			data-tauri-drag-region
			class="flex items-center justify-between px-4 pb-1 pt-3 text-xs text-muted-foreground"
		>
			<span data-tauri-drag-region class="flex items-center gap-1.5">
				{{ headingText }}
				<select
					v-model="workspaceId"
					class="rounded-md border border-border bg-transparent px-1.5 py-0.5 text-xs text-foreground outline-hidden dark:border-border dark:bg-background"
					:disabled="saving"
					@change="rememberWorkspace"
				>
					<option v-for="ws in workspaces" :key="ws.id" :value="ws.id">
						{{ ws.name }}
					</option>
				</select>
			</span>
			<div class="flex items-center gap-2">
				<span data-tauri-drag-region>esc to close</span>
				<button
					type="button"
					class="rounded-md bg-primary px-2.5 py-1 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
					:disabled="saving || !canAdd"
					data-testid="quick-add-submit"
					@click="submit"
				>
					Add ↵
				</button>
			</div>
		</div>

		<form
			class="flex min-h-0 flex-1 flex-col gap-2 px-4 pb-3"
			@submit.prevent="submit"
		>
			<div class="flex items-center gap-2 text-xs">
				<span
					class="flex overflow-hidden rounded-md border border-border dark:border-border"
				>
					<button
						v-for="option in MODES"
						:key="option.value"
						type="button"
						class="px-2 py-0.5 text-xs"
						:class="
							mode === option.value
								? 'bg-muted text-foreground dark:bg-muted dark:text-foreground'
								: 'text-muted-foreground hover:text-foreground dark:text-muted-foreground dark:hover:text-foreground'
						"
						:disabled="saving"
						:aria-pressed="mode === option.value"
						:data-testid="`quick-add-mode-${option.value}`"
						@click="mode = option.value"
					>
						{{ option.label }}
					</button>
				</span>
				<template v-if="isTask">
					<select
						v-model="categoryId"
						class="min-w-0 max-w-[16rem] rounded-md border border-border bg-transparent px-1.5 py-0.5 text-xs text-foreground outline-hidden dark:border-border dark:bg-background"
						:disabled="saving || categoriesLoading"
						aria-label="Category"
						data-testid="quick-add-category"
					>
						<option :value="null">No category</option>
						<option
							v-for="option in categoryOptions"
							:key="option.id"
							:value="option.id"
						>
							{{ option.label }}
						</option>
					</select>
					<span
						v-if="categoriesError"
						class="text-destructive dark:text-destructive"
						data-testid="quick-add-categories-error"
					>
						Could not load categories
					</span>
				</template>
			</div>

			<div v-if="mode === 'page'" class="flex min-h-0 flex-1 flex-col gap-2">
				<div class="flex items-center gap-2 text-xs">
					<input
						v-model="pageQuery"
						data-selectable
						class="w-28 rounded-md border border-border bg-transparent px-1.5 py-1 outline-hidden dark:border-border"
						placeholder="Find page"
						:disabled="saving"
					/>
					<select
						v-model="pageId"
						class="min-w-0 flex-1 rounded-md border border-border bg-transparent px-1.5 py-1 outline-hidden dark:border-border dark:bg-background"
						:disabled="saving || !pageOptions.length"
						data-testid="quick-add-page"
					>
						<option :value="null" disabled>
							{{ pagesLoading ? 'Loading pages…' : 'Choose a page' }}
						</option>
						<option
							v-for="option in visiblePageOptions"
							:key="option.id"
							:value="option.id"
						>
							{{ '\u00a0\u00a0'.repeat(option.depth) }}{{ option.title }}
						</option>
					</select>
					<select
						v-model="section"
						class="w-40 rounded-md border border-border bg-transparent px-1.5 py-1 outline-hidden dark:border-border dark:bg-background"
						:disabled="saving || !pageId"
						data-testid="quick-add-section"
					>
						<option value="">End of page</option>
						<option v-for="heading in sections" :key="heading" :value="heading">
							{{ heading }}
						</option>
					</select>
				</div>
				<textarea
					ref="pageTextInput"
					v-model="pageText"
					data-selectable
					class="min-h-0 flex-1 resize-none rounded-md bg-muted/50 p-2 text-sm outline-hidden dark:bg-muted/30"
					placeholder="What to add to the page?"
					:disabled="saving"
				/>
			</div>
			<div v-else class="flex items-baseline gap-2">
				<span
					v-if="prefixText"
					class="max-w-[40%] shrink-0 truncate text-xl font-medium text-muted-foreground dark:text-muted-foreground"
					data-testid="quick-add-prefix"
				>
					{{ prefixText }}
				</span>
				<input
					ref="titleInput"
					v-model="title"
					data-selectable
					class="w-full min-w-0 bg-transparent text-xl font-medium outline-hidden placeholder:text-muted-foreground"
					placeholder="What needs to be done?"
					:disabled="saving"
				/>
			</div>
			<textarea
				v-if="mode !== 'page' && (note || showNote)"
				v-model="note"
				data-selectable
				class="min-h-0 flex-1 resize-none rounded-md bg-muted/50 p-2 text-sm outline-hidden dark:bg-muted/30"
				placeholder="Description"
				:disabled="saving"
			/>
			<div
				v-if="screenshotUrl && showsScreenshot"
				class="flex min-h-0 flex-1 items-start gap-2"
			>
				<img
					:src="screenshotUrl"
					alt="Screenshot"
					class="max-h-full max-w-[60%] rounded-md border border-border object-contain dark:border-border"
				/>
				<button
					type="button"
					class="text-xs text-muted-foreground hover:text-foreground"
					@click="dropScreenshot"
				>
					Remove
				</button>
			</div>
			<div
				v-else-if="mode === 'screenshot'"
				class="flex items-center gap-3 text-xs"
			>
				<button
					type="button"
					class="rounded-md border border-border px-2 py-1 text-foreground hover:bg-muted dark:border-border dark:hover:bg-muted"
					:disabled="saving"
					data-testid="quick-add-capture"
					@click="captureHere"
				>
					Capture screenshot
				</button>
				<button
					type="button"
					class="rounded-md border border-border px-2 py-1 text-foreground hover:bg-muted dark:border-border dark:hover:bg-muted"
					:disabled="saving"
					data-testid="quick-add-choose-image"
					@click="fileInput.click()"
				>
					Choose image
				</button>
				<span class="text-muted-foreground dark:text-muted-foreground">
					Capture or choose a screenshot
				</span>
			</div>

			<div class="mt-auto flex items-center gap-3 text-xs">
				<button
					v-if="mode !== 'page' && !note && !showNote"
					type="button"
					class="text-muted-foreground hover:text-foreground"
					@click="showNote = true"
				>
					+ Description
				</button>
				<button
					v-if="pendingScreenshotUrl"
					type="button"
					class="text-muted-foreground hover:text-foreground dark:text-muted-foreground dark:hover:text-foreground"
					:disabled="saving"
					data-testid="quick-add-use-new-screenshot"
					@click="usePendingScreenshot"
				>
					Use new screenshot
				</button>
				<button
					v-if="draftStarted"
					type="button"
					class="text-muted-foreground hover:text-destructive dark:text-muted-foreground dark:hover:text-destructive"
					:disabled="saving"
					data-testid="quick-add-discard"
					@click="discard"
				>
					Discard
				</button>
				<span
					v-if="screenshotUnsent"
					class="text-muted-foreground dark:text-muted-foreground"
					data-testid="quick-add-unsent-hint"
				>
					Screenshot kept, but not sent with a routine or task. Adding discards
					it.
				</span>
				<span
					v-if="message"
					:class="error ? 'text-destructive' : 'text-emerald-500'"
				>
					{{ message }}
				</span>
			</div>
		</form>
	</div>
</template>

<script>
	import { getCategoriesOfWorkspace } from '@/actions/tmgr/categories';
	import { createDailyTask } from '@/actions/tmgr/daily-tasks';
	import { uploadPageFile, uploadTaskFile } from '@/actions/tmgr/files';
	import { appendToPage, getPage, getPagesTree } from '@/actions/tmgr/pages';
	import { getStatusesOfWorkspace } from '@/actions/tmgr/statuses';
	import { createTask, getTasksIndexes } from '@/actions/tmgr/tasks';
	import { getWorkspaces } from '@/actions/tmgr/workspaces';
	import {
		activeLocalWorkspace,
		followActiveLocalWorkspace,
	} from '@/local/runtime';
	import store from '@/store';
	import {
		activeCategories,
		buildCategoryGroups,
		flattenGroups,
		settingValue,
		TITLE_PATTERN_KEY,
	} from '@/utils/categorySettingsPage';
	import { pickDefaultStatusId } from '@/utils/defaultStatus';
	import {
		pickQuickAddWorkspace,
		splitQuickText,
	} from '@/utils/desktopShortcuts';
	import {
		appendText,
		canSubmitDraft,
		DEFAULT_TYPE,
		hasDraft,
		isScreenshotUnsent,
		isTaskType,
		pickRememberedCategory,
		planOpen,
		readLastCategory,
		submitKind,
		typeAfterScreenshot,
		writeLastCategory,
	} from '@/utils/quickAddDraft';
	import {
		appendToLoadedPage,
		filterPageOptions,
		flattenPages,
		listPageSections,
		pickRememberedPage,
		readLastPage,
		writeLastPage,
	} from '@/utils/quickAddPage';
	import { relayCreated, relayPageAppended } from '@/utils/quickAddPageRelay';
	import {
		alreadyPrefixed,
		composePatternTitle,
		hasIndexToken,
		prefixPreview,
		renderPrefix,
	} from '@/utils/quickAddTitle';
	import { requestCache } from '@/utils/requestCache';
	import { format } from 'date-fns';
	import {
		computed,
		defineComponent,
		nextTick,
		onMounted,
		ref,
		watch,
	} from 'vue';

	const MODES = [
		{ value: 'routine', label: 'Routine' },
		{ value: 'task', label: 'Task' },
		{ value: 'screenshot', label: 'Task + screenshot' },
		{ value: 'page', label: 'Page' },
	];

	const HEADINGS = {
		routine: 'Add to Daily Routines in',
		task: 'Add task to backlog in',
		screenshot: 'Add task with screenshot to backlog in',
		page: 'Add to a page in',
	};

	const invoke = async (command, args) => {
		const core = await import('@tauri-apps/api/core');
		return core.invoke(command, args);
	};

	const WORKSPACE_KEY = 'desktop.quickAdd.workspaceId';

	const rememberedWorkspace = () => {
		try {
			return Number(localStorage.getItem(WORKSPACE_KEY)) || null;
		} catch {
			return null;
		}
	};

	const reportFailure = async (e) => {
		const config = e?.config;
		const status = e?.response?.status;
		const detail = e?.response?.data?.message || e?.message || String(e);
		try {
			const log = await import('@tauri-apps/plugin-log');
			await log.warn(
				`[quick-add] failed ${config?.method?.toUpperCase() ?? ''} ${
					config?.url ?? ''
				} status=${status ?? '-'}: ${detail}`,
			);
		} catch {
			/* logging must never break quick add */
		}
	};

	const ACCESSIBILITY_HINT =
		'Allow TMGR in System Settings → Privacy & Security → Accessibility, then try again.';

	export default defineComponent({
		name: 'QuickAdd',
		setup() {
			const title = ref('');
			const note = ref('');
			const showNote = ref(false);
			const screenshot = ref(null);
			const screenshotUrl = ref('');
			const workspaceId = ref(null);
			const workspaces = ref([]);
			const mode = ref(DEFAULT_TYPE);
			const pageText = ref('');
			const pageQuery = ref('');
			const pageId = ref(null);
			const pageOptions = ref([]);
			const pagesLoading = ref(false);
			const section = ref('');
			const sections = ref([]);
			const pageTextInput = ref(null);
			const fileInput = ref(null);
			const categories = ref([]);
			const categoryId = ref(null);
			const categoriesLoading = ref(false);
			const categoriesError = ref(false);
			const prefix = ref('');
			let pagesRequest = 0;
			let pagesWorkspace = null;
			let sectionsRequest = 0;
			let categoriesRequest = 0;
			let categoriesWorkspace = null;
			let categoriesPending = null;
			let categoriesStale = false;
			let prefixRequest = 0;
			let resumeDraft = false;

			const isTask = computed(() => isTaskType(mode.value));
			const headingText = computed(() => HEADINGS[mode.value]);
			const visiblePageOptions = computed(() =>
				filterPageOptions(pageOptions.value, pageQuery.value, pageId.value),
			);
			const canSubmit = computed(() =>
				canSubmitDraft(
					mode.value,
					{
						title: title.value,
						pageText: pageText.value,
						pageId: pageId.value,
					},
					Boolean(screenshot.value),
				),
			);
			const canAdd = computed(
				() => canSubmit.value && !(isTask.value && categoriesLoading.value),
			);
			const showsScreenshot = computed(
				() => mode.value === 'screenshot' || mode.value === 'page',
			);
			const screenshotUnsent = computed(() =>
				isScreenshotUnsent(mode.value, Boolean(screenshot.value)),
			);
			const categoryOptions = computed(() =>
				flattenGroups(buildCategoryGroups(categories.value)).map((node) => ({
					id: node.category.id,
					label: `${'\u00a0\u00a0'.repeat(node.depth)}${node.category.title}`,
				})),
			);
			const pattern = computed(() =>
				isTask.value
					? settingValue(
							categories.value.find((c) => c.id === categoryId.value),
							TITLE_PATTERN_KEY,
						).trim()
					: '',
			);
			const prefixText = computed(() =>
				prefix.value && !alreadyPrefixed(pattern.value, title.value)
					? prefixPreview(prefix.value)
					: '',
			);
			const selectedWorkspace = () =>
				workspaces.value.find((ws) => ws.id === workspaceId.value);
			const useWorkspaceForPages = (ws) => {
				followActiveLocalWorkspace(Boolean(ws?.is_local));
				store.commit('updateUserWorkspaceSetting', { workspaceId: ws.id });
			};

			const loadPages = async () => {
				const ws = selectedWorkspace();
				const request = ++pagesRequest;
				pageId.value = null;
				pageOptions.value = [];
				pagesWorkspace = null;
				if (!ws) return;
				pagesLoading.value = true;
				try {
					useWorkspaceForPages(ws);
					const options = flattenPages(await getPagesTree(false));
					if (request !== pagesRequest) return;
					pageOptions.value = options;
					pagesWorkspace = { id: ws.id, code: ws.code };
					pageId.value = pickRememberedPage(options, readLastPage(ws.id));
				} catch (e) {
					if (request !== pagesRequest) return;
					error.value = true;
					message.value =
						e?.response?.data?.error === 'feature_disabled'
							? 'Pages are disabled in this workspace.'
							: 'Could not load pages.';
				} finally {
					if (request === pagesRequest) pagesLoading.value = false;
				}
			};

			const loadSections = async (id) => {
				const request = ++sectionsRequest;
				section.value = '';
				sections.value = [];
				if (!id) return;
				try {
					const page = await getPage(id);
					if (request === sectionsRequest) {
						sections.value = listPageSections(page.body);
					}
				} catch {
					/* the page can still be appended to at its end */
				}
			};

			const rememberedCategory = () =>
				categoriesWorkspace === workspaceId.value
					? pickRememberedCategory(
							categories.value,
							readLastCategory(workspaceId.value),
						)
					: null;

			const forgetCategories = () => {
				categoriesRequest++;
				categoriesStale = false;
				categories.value = [];
				categoriesWorkspace = null;
				categoriesPending = null;
				categoriesLoading.value = false;
				categoryId.value = null;
			};

			/** The window is only hidden between opens, so a fresh open refetches: another window may have changed the categories. */
			const loadCategories = async (keepSelection = false) => {
				const ws = selectedWorkspace();
				const request = ++categoriesRequest;
				const previous = categoryId.value;
				categoriesStale = false;
				if (!keepSelection) {
					categories.value = [];
					categoriesWorkspace = null;
					categoryId.value = null;
				}
				categoriesPending = ws?.id ?? null;
				categoriesError.value = false;
				if (!ws) return;
				categoriesLoading.value = true;
				try {
					followActiveLocalWorkspace(Boolean(ws.is_local));
					requestCache.invalidate(`categories-workspace-${ws.id}`);
					const list = activeCategories(await getCategoriesOfWorkspace(ws.id));
					if (request !== categoriesRequest) return;
					categories.value = list;
					categoriesWorkspace = ws.id;
					categoryId.value =
						keepSelection && list.some((c) => c.id === previous)
							? previous
							: keepSelection
								? null
								: rememberedCategory();
				} catch (e) {
					if (request !== categoriesRequest) return;
					console.error('quick add: categories not loaded', e);
					categoriesError.value = true;
				} finally {
					if (request === categoriesRequest) {
						categoriesLoading.value = false;
						categoriesPending = null;
					}
				}
			};

			const ensureCategories = () => {
				const id = workspaceId.value;
				if (!isTask.value || id == null || categoriesPending === id) return;
				if (categoriesWorkspace === id) {
					if (categoriesStale) void loadCategories(true);
					return;
				}
				void loadCategories();
			};

			const fetchIndexes = () => {
				followActiveLocalWorkspace(Boolean(selectedWorkspace()?.is_local));
				return getTasksIndexes(categoryId.value, workspaceId.value);
			};

			const refreshPrefix = async (keepShown = false) => {
				const request = ++prefixRequest;
				const current = pattern.value;
				if (!keepShown || !current) prefix.value = '';
				if (!current || workspaceId.value == null) return;
				if (!hasIndexToken(current)) {
					prefix.value = renderPrefix(current);
					return;
				}
				try {
					const indexes = await fetchIndexes();
					if (request === prefixRequest) {
						prefix.value = renderPrefix(current, indexes);
					}
				} catch {
					/* the preview stays hidden; submit asks for the number again */
				}
			};

			watch(pageId, loadSections);
			watch(mode, async (next) => {
				if (next !== 'page') return;
				message.value = '';
				error.value = false;
				if (!pageOptions.value.length) void loadPages();
				await nextTick();
				pageTextInput.value?.focus();
			});
			watch(workspaceId, () => {
				pageOptions.value = [];
				pageId.value = null;
				if (mode.value === 'page') void loadPages();
				forgetCategories();
				ensureCategories();
			});
			watch(isTask, ensureCategories);
			watch([pattern, categoryId, workspaceId], () => refreshPrefix());

			const rememberWorkspace = () => {
				try {
					localStorage.setItem(WORKSPACE_KEY, String(workspaceId.value));
				} catch {
					/* storage unavailable: falls back to the current workspace */
				}
			};
			const saving = ref(false);
			const message = ref('');
			const error = ref(false);
			const titleInput = ref(null);

			const pendingScreenshotUrl = ref('');
			let pendingScreenshot = null;

			const dropScreenshot = () => {
				if (screenshotUrl.value) URL.revokeObjectURL(screenshotUrl.value);
				screenshot.value = null;
				screenshotUrl.value = '';
			};

			const dropPendingScreenshot = () => {
				if (pendingScreenshotUrl.value) {
					URL.revokeObjectURL(pendingScreenshotUrl.value);
				}
				pendingScreenshot = null;
				pendingScreenshotUrl.value = '';
			};

			const attachScreenshot = (file, url) => {
				dropScreenshot();
				screenshot.value = file;
				screenshotUrl.value = url;
				mode.value = typeAfterScreenshot(mode.value);
			};

			const usePendingScreenshot = () => {
				if (!pendingScreenshot) return;
				attachScreenshot(pendingScreenshot, pendingScreenshotUrl.value);
				pendingScreenshot = null;
				pendingScreenshotUrl.value = '';
				message.value = '';
			};

			const chooseImage = (event) => {
				const file = event.target.files?.[0];
				event.target.value = '';
				if (!file) return;
				attachScreenshot(file, URL.createObjectURL(file));
				message.value = '';
				error.value = false;
			};

			const readCapture = async (path) => {
				const bytes = await invoke('take_capture', { path });
				const blob = new Blob([bytes], { type: 'image/png' });
				return {
					file: new File(
						[blob],
						`screenshot-${format(new Date(), 'yyyy-MM-dd-HHmmss')}.png`,
						{ type: 'image/png' },
					),
					url: URL.createObjectURL(blob),
				};
			};

			const draftStarted = computed(() =>
				hasDraft(
					{
						mode: mode.value,
						title: title.value,
						note: note.value,
						pageText: pageText.value,
					},
					Boolean(screenshot.value),
				),
			);

			const focusField = () =>
				(mode.value === 'page' ? pageTextInput : titleInput).value?.focus();

			const reset = () => {
				mode.value = DEFAULT_TYPE;
				pageText.value = '';
				pageQuery.value = '';
				pageOptions.value = [];
				pageId.value = null;
				title.value = '';
				note.value = '';
				showNote.value = false;
				message.value = '';
				error.value = false;
				forgetCategories();
				dropScreenshot();
				dropPendingScreenshot();
			};

			const discard = async () => {
				reset();
				await nextTick();
				focusField();
			};

			const hide = () => invoke('hide_quick_add');

			const captureHere = async () => {
				if (saving.value) return;
				resumeDraft = true;
				let path = null;
				try {
					await hide();
					path = await invoke('capture_screenshot');
				} catch (e) {
					console.error('quick add: capture failed', e);
					void reportFailure(e);
				}
				try {
					await invoke('open_quick_add', {
						payload: {
							workspaceId: workspaceId.value,
							...(path ? { screenshot: path } : {}),
						},
					});
				} catch (e) {
					resumeDraft = false;
					console.error('quick add: reopen failed', e);
				}
			};

			const load = async () => {
				const payload = await invoke('take_quick_add');
				if (payload === null || payload === undefined) return;
				const planned = planOpen(
					{
						hasDraft: draftStarted.value,
						hasScreenshot: Boolean(screenshot.value),
					},
					payload,
				);
				const plan = resumeDraft ? { ...planned, keep: true } : planned;
				resumeDraft = false;
				if (plan.keep) {
					message.value = '';
					error.value = false;
				} else {
					reset();
				}
				followActiveLocalWorkspace();
				try {
					const loaded = await getWorkspaces();
					// A local workspace other than the active one would 409 on submit
					// (local writes are refused outside the workspace that is open).
					const active = activeLocalWorkspace();
					workspaces.value = loaded.filter(
						(ws) => !ws.is_local || ws.id === active?.id,
					);
				} catch (e) {
					console.error('quick add: workspaces not loaded', e);
				}
				workspaceId.value = pickQuickAddWorkspace(
					workspaces.value,
					plan.keep ? workspaceId.value : rememberedWorkspace(),
					payload?.workspaceId ?? null,
				);
				if (plan.text === 'set') {
					const split = splitQuickText(payload.text);
					title.value = split.title;
					note.value = split.note;
					pageText.value = payload.text;
				} else if (plan.text === 'append') {
					if (mode.value === 'page') {
						pageText.value = appendText(pageText.value, payload.text);
					} else if (!title.value.trim()) {
						const split = splitQuickText(payload.text);
						title.value = split.title;
						note.value = appendText(note.value, split.note);
					} else {
						note.value = appendText(note.value, payload.text);
					}
					message.value = 'Selected text added to your draft';
				}
				if (plan.screenshot) {
					const capture = await readCapture(payload.screenshot);
					if (plan.screenshot === 'attach') {
						dropPendingScreenshot();
						attachScreenshot(capture.file, capture.url);
					} else {
						dropPendingScreenshot();
						pendingScreenshot = capture.file;
						pendingScreenshotUrl.value = capture.url;
						message.value = 'Draft kept. New screenshot captured';
					}
				}
				if (payload?.error === 'accessibility') {
					error.value = true;
					message.value = ACCESSIBILITY_HINT;
				}
				if (plan.keep && categoriesWorkspace === workspaceId.value) {
					if (isTask.value) void loadCategories(true);
					else categoriesStale = true;
				} else {
					ensureCategories();
				}
				void refreshPrefix(true);
				await nextTick();
				focusField();
			};

			const submitPage = async () => {
				if (!canSubmit.value || saving.value) return;
				saving.value = true;
				error.value = false;
				message.value = 'Adding…';
				const ws = selectedWorkspace();
				try {
					if (!pagesWorkspace || pagesWorkspace.id !== ws?.id) {
						throw new Error('The page list belongs to another workspace');
					}
					useWorkspaceForPages(ws);
					await appendToLoadedPage(
						pagesWorkspace,
						pageId.value,
						{
							text: pageText.value,
							section: section.value,
							screenshot: screenshot.value,
						},
						{ uploadPageFile, appendToPage, relayPageAppended },
					);
					writeLastPage(ws.id, pageId.value);
					message.value = 'Added';
					setTimeout(async () => {
						await hide();
						reset();
					}, 500);
				} catch (e) {
					console.error('quick add to page failed', e);
					void reportFailure(e);
					error.value = true;
					message.value =
						e?.response?.data?.message ||
						'Could not add to the page. Try again.';
				} finally {
					saving.value = false;
				}
			};

			const composeTaskTitle = async () => {
				const current = pattern.value;
				if (!current) return title.value.trim();
				let indexes = {};
				if (hasIndexToken(current)) {
					try {
						indexes = await fetchIndexes();
					} catch (e) {
						e.userMessage = 'Could not get the next task number. Try again.';
						throw e;
					}
				}
				return composePatternTitle(
					current,
					renderPrefix(current, indexes),
					title.value,
				);
			};

			const submit = async () => {
				if (mode.value === 'page') return submitPage();
				if (!canAdd.value || saving.value) return;
				saving.value = true;
				error.value = false;
				message.value = 'Adding…';
				const targetWorkspace = workspaces.value.find(
					(ws) => ws.id === workspaceId.value,
				);
				followActiveLocalWorkspace(Boolean(targetWorkspace?.is_local));
				try {
					const kind = submitKind(mode.value);
					const fields = {
						title: title.value.trim(),
						description: note.value.trim() || undefined,
						workspace_id: workspaceId.value || undefined,
					};
					if (kind === 'routine') {
						const routine = await createDailyTask(fields);
						if (targetWorkspace?.is_local) {
							try {
								const { emitTo } = await import('@tauri-apps/api/event');
								await emitTo('main', 'quick-add://routine-created', {
									workspaceId: routine?.workspace_id ?? workspaceId.value,
									routineId: routine?.id,
								});
							} catch (e) {
								console.error('quick add: routine-created relay failed', e);
							}
						}
						void relayCreated('routine');
					} else {
						const task = await createTask({
							...fields,
							title: await composeTaskTitle(),
							project_category_id: categoryId.value ?? undefined,
							status: 'created',
							status_id: pickDefaultStatusId(
								await getStatusesOfWorkspace(workspaceId.value),
							),
							is_daily_routine: false,
						});
						if (categoriesWorkspace === workspaceId.value) {
							writeLastCategory(workspaceId.value, categoryId.value);
						}
						try {
							if (kind === 'task-screenshot') {
								await uploadTaskFile(task.id, screenshot.value);
							}
						} finally {
							void relayCreated('task');
						}
					}
					message.value = 'Added';
					setTimeout(async () => {
						await hide();
						reset();
					}, 500);
				} catch (e) {
					console.error('quick add failed', e);
					void reportFailure(e);
					error.value = true;
					message.value =
						e?.userMessage ||
						e?.response?.data?.message ||
						'Could not add the task. Try again.';
				} finally {
					saving.value = false;
				}
			};

			const COMPACT = 160;
			const EXPANDED = 340;
			watch(
				() =>
					Boolean(
						mode.value === 'page' ||
						mode.value === 'screenshot' ||
						note.value ||
						showNote.value,
					),
				async (expanded) => {
					const { getCurrentWindow, LogicalSize } =
						await import('@tauri-apps/api/window');
					await getCurrentWindow().setSize(
						new LogicalSize(620, expanded ? EXPANDED : COMPACT),
					);
				},
				{ immediate: true },
			);

			onMounted(async () => {
				const { listen } = await import('@tauri-apps/api/event');
				await listen('quick-add://open', load);
				await load();
				titleInput.value?.focus();
			});

			return {
				title,
				note,
				showNote,
				screenshotUrl,
				saving,
				message,
				error,
				titleInput,
				pageTextInput,
				fileInput,
				MODES,
				mode,
				headingText,
				isTask,
				pageText,
				pageQuery,
				pageId,
				pageOptions,
				visiblePageOptions,
				pagesLoading,
				section,
				sections,
				canSubmit,
				canAdd,
				workspaceId,
				workspaces,
				categoryId,
				categoryOptions,
				categoriesLoading,
				categoriesError,
				prefixText,
				showsScreenshot,
				screenshotUnsent,
				rememberWorkspace,
				hide,
				submit,
				dropScreenshot,
				pendingScreenshotUrl,
				usePendingScreenshot,
				chooseImage,
				captureHere,
				draftStarted,
				discard,
			};
		},
	});
</script>
