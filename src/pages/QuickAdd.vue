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
				{{ screenshotUrl ? 'Add task with screenshot to backlog in' : 'Add to Daily Routines in' }}
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
				<button
					type="button"
					class="rounded-md bg-primary px-2.5 py-1 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
					:disabled="saving || !title.trim()"
					@click="submit"
				>
					Add ↵
				</button>
			</div>
		</div>

		<form class="flex min-h-0 flex-1 flex-col gap-2 px-4 pb-3" @submit.prevent="submit">
			<input
				ref="titleInput"
				v-model="title"
				data-selectable
				class="w-full bg-transparent text-xl font-medium outline-none placeholder:text-muted-foreground"
				placeholder="What needs to be done?"
				:disabled="saving"
			/>
			<textarea
				v-if="note || showNote"
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
					v-if="!note && !showNote"
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
	import { uploadTaskFile } from '@/actions/tmgr/files';
	import { getStatusesOfWorkspace } from '@/actions/tmgr/statuses';
	import { createTask } from '@/actions/tmgr/tasks';
	import { getWorkspaces } from '@/actions/tmgr/workspaces';
	import { pickDefaultStatusId } from '@/utils/defaultStatus';
	import {
		pickQuickAddWorkspace,
		splitQuickText,
	} from '@/utils/desktopShortcuts';
	import { format } from 'date-fns';
	import { defineComponent, nextTick, onMounted, ref, watch } from 'vue';

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
				try {
					workspaces.value = await getWorkspaces();
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

			const submit = async () => {
				if (!title.value.trim() || saving.value) return;
				saving.value = true;
				error.value = false;
				message.value = 'Adding…';
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
						await createDailyTask(fields);
					}
					message.value = 'Added';
					setTimeout(async () => {
						await hide();
						reset();
					}, 500);
				} catch (e) {
					console.error('quick add failed', e);
					error.value = true;
					message.value = 'Could not add the task. Try again.';
				} finally {
					saving.value = false;
				}
			};

			const COMPACT = 132;
			const EXPANDED = 320;
			watch(
				() => Boolean(note.value || showNote.value || screenshotUrl.value),
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
