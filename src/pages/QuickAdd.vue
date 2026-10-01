<template>
	<div
		class="flex h-screen flex-col overflow-hidden border border-border bg-background text-foreground dark:border-border dark:bg-background"
		@keydown.esc.prevent="hide"
	>
		<div
			data-tauri-drag-region
			class="flex items-center justify-between px-4 pb-1 pt-3 text-xs text-muted-foreground"
		>
			<span data-tauri-drag-region class="flex items-center gap-1.5">
				{{
					mode === 'page'
						? 'Add to a page in'
						: screenshotUrl
						? 'Add task with screenshot to backlog in'
						: 'Add to Daily Routines in'
				}}
				<select
					v-model="workspaceId"
					class="rounded-md border border-border bg-transparent px-1.5 py-0.5 text-xs text-foreground outline-none dark:border-border dark:bg-background"
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
				<span class="flex overflow-hidden rounded-md border border-border dark:border-border">
					<button
						v-for="option in MODES"
						:key="option.value"
						type="button"
						class="px-2 py-0.5 text-xs"
						:class="
							mode === option.value
								? 'bg-muted text-foreground'
								: 'text-muted-foreground hover:text-foreground'
						"
						:disabled="saving"
						:data-testid="`quick-add-mode-${option.value}`"
						@click="mode = option.value"
					>
						{{ option.label }}
					</button>
				</span>
				<button
					type="button"
					class="rounded-md bg-primary px-2.5 py-1 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
					:disabled="saving || !canSubmit"
					@click="submit"
				>
					Add ↵
				</button>
			</div>
		</div>

		<form class="flex min-h-0 flex-1 flex-col gap-2 px-4 pb-3" @submit.prevent="submit">
			<div v-if="mode === 'page'" class="flex min-h-0 flex-1 flex-col gap-2">
				<div class="flex items-center gap-2 text-xs">
					<input
						v-model="pageQuery"
						data-selectable
						class="w-28 rounded-md border border-border bg-transparent px-1.5 py-1 outline-none dark:border-border"
						placeholder="Find page"
						:disabled="saving"
					/>
					<select
						v-model="pageId"
						class="min-w-0 flex-1 rounded-md border border-border bg-transparent px-1.5 py-1 outline-none dark:border-border dark:bg-background"
						:disabled="saving || !pageOptions.length"
						data-testid="quick-add-page"
					>
						<option :value="null" disabled>
							{{ pagesLoading ? 'Loading pages…' : 'Choose a page' }}
						</option>
						<option v-for="option in visiblePageOptions" :key="option.id" :value="option.id">
							{{ '\u00a0\u00a0'.repeat(option.depth) }}{{ option.title }}
						</option>
					</select>
					<select
						v-model="section"
						class="w-40 rounded-md border border-border bg-transparent px-1.5 py-1 outline-none dark:border-border dark:bg-background"
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
					class="min-h-0 flex-1 resize-none rounded-md bg-muted/50 p-2 text-sm outline-none dark:bg-muted/30"
					placeholder="What to add to the page?"
					:disabled="saving"
				/>
			</div>
			<input
				v-else
				ref="titleInput"
				v-model="title"
				data-selectable
				class="w-full bg-transparent text-xl font-medium outline-none placeholder:text-muted-foreground"
				placeholder="What needs to be done?"
				:disabled="saving"
			/>
			<textarea
				v-if="mode === 'task' && (note || showNote)"
				v-model="note"
				data-selectable
				class="min-h-0 flex-1 resize-none rounded-md bg-muted/50 p-2 text-sm outline-none dark:bg-muted/30"
				placeholder="Description"
				:disabled="saving"
			/>
			<div v-if="screenshotUrl" class="flex min-h-0 flex-1 items-start gap-2">
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

			<div class="mt-auto flex items-center gap-3 text-xs">
				<button
					v-if="mode === 'task' && !note && !showNote"
					type="button"
					class="text-muted-foreground hover:text-foreground"
					@click="showNote = true"
				>
					+ Description
				</button>
				<span v-if="message" :class="error ? 'text-destructive' : 'text-emerald-500'">
					{{ message }}
				</span>
			</div>
		</form>
	</div>
</template>

<script>
	import { createDailyTask } from '@/actions/tmgr/daily-tasks';
	import { uploadPageFile, uploadTaskFile } from '@/actions/tmgr/files';
	import { appendToPage, getPage, getPagesTree } from '@/actions/tmgr/pages';
	import { getStatusesOfWorkspace } from '@/actions/tmgr/statuses';
	import { createTask } from '@/actions/tmgr/tasks';
	import { getWorkspaces } from '@/actions/tmgr/workspaces';
	import {
		activeLocalWorkspace,
		followActiveLocalWorkspace,
	} from '@/local/runtime';
	import store from '@/store';
	import { relayPageAppended } from '@/utils/quickAddPageRelay';
	import { pickDefaultStatusId } from '@/utils/defaultStatus';
	import {
		pickQuickAddWorkspace,
		splitQuickText,
	} from '@/utils/desktopShortcuts';
	import {
		composeAppendMarkdown,
		filterPageOptions,
		flattenPages,
		listPageSections,
		pickRememberedPage,
		readLastPage,
		writeLastPage,
	} from '@/utils/quickAddPage';
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
		{ value: 'task', label: 'Task' },
		{ value: 'page', label: 'Page' },
	];

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
				`[quick-add] failed ${config?.method?.toUpperCase() ?? ''} ${config?.url ?? ''} status=${status ?? '-'}: ${detail}`,
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
			const mode = ref('task');
			const pageText = ref('');
			const pageQuery = ref('');
			const pageId = ref(null);
			const pageOptions = ref([]);
			const pagesLoading = ref(false);
			const section = ref('');
			const sections = ref([]);
			const pageTextInput = ref(null);
			let pagesRequest = 0;
			let sectionsRequest = 0;

			const visiblePageOptions = computed(() =>
				filterPageOptions(pageOptions.value, pageQuery.value, pageId.value),
			);
			const canSubmit = computed(() =>
				mode.value === 'page'
					? Boolean(pageId.value && (pageText.value.trim() || screenshot.value))
					: Boolean(title.value.trim()),
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
				if (!ws) return;
				pagesLoading.value = true;
				try {
					useWorkspaceForPages(ws);
					const options = flattenPages(await getPagesTree(false));
					if (request !== pagesRequest) return;
					pageOptions.value = options;
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
			});

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

			const dropScreenshot = () => {
				if (screenshotUrl.value) URL.revokeObjectURL(screenshotUrl.value);
				screenshot.value = null;
				screenshotUrl.value = '';
			};

			const reset = () => {
				mode.value = 'task';
				pageText.value = '';
				pageQuery.value = '';
				pageOptions.value = [];
				pageId.value = null;
				title.value = '';
				note.value = '';
				showNote.value = false;
				message.value = '';
				error.value = false;
				dropScreenshot();
			};

			const hide = () => invoke('hide_quick_add');

			const load = async () => {
				const payload = await invoke('take_quick_add');
				if (payload === null || payload === undefined) return;
				reset();
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
					rememberedWorkspace(),
					payload?.workspaceId ?? null,
				);
				if (payload?.text) {
					const split = splitQuickText(payload.text);
					title.value = split.title;
					note.value = split.note;
					pageText.value = payload.text;
				}
				if (payload?.screenshot) {
					const bytes = await invoke('take_capture', { path: payload.screenshot });
					const blob = new Blob([bytes], { type: 'image/png' });
					screenshot.value = new File(
						[blob],
						`screenshot-${format(new Date(), 'yyyy-MM-dd-HHmmss')}.png`,
						{ type: 'image/png' },
					);
					screenshotUrl.value = URL.createObjectURL(blob);
				}
				if (payload?.error === 'accessibility') {
					error.value = true;
					message.value = ACCESSIBILITY_HINT;
				}
				await nextTick();
				titleInput.value?.focus();
			};

			const submitPage = async () => {
				if (!canSubmit.value || saving.value) return;
				saving.value = true;
				error.value = false;
				message.value = 'Adding…';
				const ws = selectedWorkspace();
				try {
					useWorkspaceForPages(ws);
					const fileId = screenshot.value
						? (await uploadPageFile(pageId.value, screenshot.value)).id
						: null;
					const updated = await appendToPage(pageId.value, {
						markdown: composeAppendMarkdown(pageText.value, fileId),
						heading: section.value || undefined,
					});
					void relayPageAppended({
						workspace_code: ws.code,
						page: {
							id: updated.id,
							slug: updated.slug,
							title: updated.title,
							version: updated.version,
						},
					}).catch((e) => console.error('quick add: page-appended relay failed', e));
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
						e?.response?.data?.message || 'Could not add to the page. Try again.';
				} finally {
					saving.value = false;
				}
			};

			const submit = async () => {
				if (mode.value === 'page') return submitPage();
				if (!title.value.trim() || saving.value) return;
				saving.value = true;
				error.value = false;
				message.value = 'Adding…';
				const targetWorkspace = workspaces.value.find(
					(ws) => ws.id === workspaceId.value,
				);
				followActiveLocalWorkspace(Boolean(targetWorkspace?.is_local));
				try {
					const fields = {
						title: title.value.trim(),
						description: note.value.trim() || undefined,
						workspace_id: workspaceId.value || undefined,
					};
					if (screenshot.value) {
						const task = await createTask({
							...fields,
							status: 'created',
							status_id: pickDefaultStatusId(
								await getStatusesOfWorkspace(workspaceId.value),
							),
							is_daily_routine: false,
						});
						await uploadTaskFile(task.id, screenshot.value);
					} else {
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
						e?.response?.data?.message || 'Could not add the task. Try again.';
				} finally {
					saving.value = false;
				}
			};

			const COMPACT = 132;
			const EXPANDED = 320;
			watch(
				() =>
					Boolean(
						mode.value === 'page' ||
							note.value ||
							showNote.value ||
							screenshotUrl.value,
					),
				async (expanded) => {
					const { getCurrentWindow, LogicalSize } = await import(
						'@tauri-apps/api/window'
					);
					await getCurrentWindow().setSize(
						new LogicalSize(620, expanded ? EXPANDED : COMPACT),
					);
				},
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
				MODES,
				mode,
				pageText,
				pageQuery,
				pageId,
				pageOptions,
				visiblePageOptions,
				pagesLoading,
				section,
				sections,
				canSubmit,
				workspaceId,
				workspaces,
				rememberWorkspace,
				hide,
				submit,
				dropScreenshot,
			};
		},
	});
</script>
