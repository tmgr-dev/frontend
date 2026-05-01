<template>
	<FeatureGate
		feature-key="daily_routines"
		title="Daily Routines"
		description="Create recurring tasks and track your daily habits. Build consistent routines and monitor your progress over time."
		:icon="CalendarCheck"
	>
		<template #preview>
			<DailyRoutinesPreview />
		</template>

		<BaseLayout>
			<template #body>
				<div ref="rootRef" class="relative flex h-full flex-col overflow-hidden pb-10">
					<!-- Top toolbar -->
					<div class="flex flex-col gap-3 border-b border-line px-2 pb-3 pt-3 md:px-6 md:pt-4">
						<div class="flex flex-wrap items-center gap-2.5">
							<div class="flex flex-1 shrink-0 gap-1.5">
								<CountChip :n="counts.routines" label="ROUTINES" />
								<CountChip :n="counts.completed" label="DONE" :color="'#22c55e'" />
								<CountChip v-if="!isMobile" :n="counts.archived" label="ARCHIVED" />
							</div>
							<button
								type="button"
								class="flex h-8 items-center gap-1.5 rounded-pill border border-line bg-surface px-3 text-2xs font-medium text-ink-subtle transition-colors hover:border-brand hover:text-brand"
								title="Import .ics calendar"
								@click="triggerImport"
							>
								<Upload :size="14" stroke-width="2" />
								<span v-if="!isMobile">Import</span>
							</button>
							<button
								type="button"
								class="flex h-8 items-center gap-1.5 rounded-pill border border-line bg-surface px-3 text-2xs font-medium text-ink-subtle transition-colors hover:border-brand hover:text-brand"
								title="Export .ics calendar"
								@click="onExport"
							>
								<Download :size="14" stroke-width="2" />
								<span v-if="!isMobile">Export</span>
							</button>
							<input
								ref="fileRef"
								type="file"
								accept=".ics,text/calendar"
								class="hidden"
								@change="onImportFile"
							/>
						</div>

						<div class="flex items-center gap-3 overflow-x-auto pb-1">
							<ViewSwitcher v-model="view" :compact="isTablet" />
							<template v-if="view !== 'list'">
								<button
									type="button"
									class="shrink-0 rounded-card border border-line bg-transparent px-3 py-1.5 text-2xs font-medium text-ink hover:bg-surface-hover"
									@click="goToday"
								>
									Today
								</button>
								<div class="flex shrink-0 gap-0.5">
									<button
										type="button"
										class="flex h-7 w-7 items-center justify-center rounded-md text-ink-subtle hover:bg-surface-hover hover:text-ink"
										@click="step(-1)"
									>
										<DRIcon name="chev-l" :size="13" stroke="currentColor" />
									</button>
									<button
										type="button"
										class="flex h-7 w-7 items-center justify-center rounded-md text-ink-subtle hover:bg-surface-hover hover:text-ink"
										@click="step(1)"
									>
										<DRIcon name="chev-r" :size="13" stroke="currentColor" />
									</button>
								</div>
								<span class="ml-1 whitespace-nowrap text-sm font-semibold text-ink">{{ dateLabel }}</span>
							</template>
						</div>
					</div>

					<!-- Body -->
					<div class="flex flex-1 flex-col gap-3 overflow-hidden p-2 md:p-5">
						<div v-if="isLoading && !entries.length" class="flex flex-1 flex-col gap-2">
							<SkeletonListItem v-for="n in 5" :key="n" class="rounded-card border border-line bg-surface" :show-avatar="false" />
						</div>
						<template v-else>
							<!-- Unscheduled section (visible above all calendar views) -->
							<div
								v-if="view !== 'list' && view !== 'day' && (unscheduledEntries.length || dragActive)"
								class="flex shrink-0 flex-col gap-1.5 max-h-[220px] overflow-y-auto rounded-card transition-colors"
								:class="dragHoverKey === `unsched:${todayIso}` ? 'bg-brand/10 ring-1 ring-brand/40' : ''"
								data-dr-drop
								data-dr-kind="unscheduled"
								:data-dr-date="todayIso"
							>
								<div class="flex items-center justify-between gap-2 px-1">
									<div class="text-[10px] font-bold uppercase tracking-wider text-ink-subtle">
										Unscheduled <span>{{ unscheduledEntries.length }}</span>
									</div>
									<button
										v-if="unscheduledDoneCount > 0"
										type="button"
										class="rounded-pill border border-line bg-surface px-2.5 py-1 text-[10px] font-semibold uppercase tracking-wider text-ink-subtle hover:border-brand hover:text-brand"
										@click="onArchiveDoneUnscheduled"
									>
										Archive done ({{ unscheduledDoneCount }})
									</button>
								</div>
								<RoutineRow
									v-for="e in unscheduledEntries"
									:key="`u-${e.task_id}-${e.date}`"
									class="shrink-0"
									:entry="e"
									@toggle="onToggle"
									@edit="onEdit"
									@delete="onDelete"
									@archive="onArchive"
									@select="onEdit"
									@context="onContext"
								/>
							</div>

							<ListView
								v-if="view === 'list'"
								:entries="todayEntries"
								@toggle="onToggle"
								@edit="onEdit"
								@delete="onDelete"
								@archive="onArchive"
								@archive-done-unscheduled="onArchiveDoneUnscheduled"
								@select="onEdit"
								@context="onContext"
							/>
							<DayView
								v-else-if="view === 'day'"
								:entries="dayEntries"
								:date="cursor"
								:is-mobile="isMobile"
								@toggle="onToggle"
								@edit="onEdit"
								@delete="onDelete"
								@archive="onArchive"
								@archive-done-unscheduled="onArchiveDoneUnscheduled"
								@create="onCreateAt"
								@select="onEdit"
								@context="onContext"
							/>
							<WeekView
								v-else-if="view === 'week'"
								:entries="scheduledEntries"
								:week-start="weekStart"
								:is-mobile="isMobile"
								:is-tablet="isTablet"
								@toggle="onToggle"
								@edit="onEdit"
								@create="onCreateAt"
								@context="onContext"
							/>
							<MonthView
								v-else-if="view === 'month'"
								:entries="scheduledEntries"
								:month-date="cursor"
								:is-mobile="isMobile"
								@toggle="onToggle"
								@select-day="onSelectDay"
								@create="onCreateAt"
								@context="onContext"
							/>
							<YearView
								v-else-if="view === 'year'"
								:year="cursor.getFullYear()"
								:stats="yearStats"
								:width="width"
								@select-day="onSelectDay"
							/>
						</template>
					</div>

					<EditRoutineModal
						v-if="editingRoutine"
						:routine="editingRoutine"
						@close="editingRoutine = null"
						@save="onSaveRoutine"
						@delete="onDeleteRoutine"
					/>

					<RoutineContextMenu
						v-if="contextMenu"
						:entry="contextMenu.entry"
						:x="contextMenu.x"
						:y="contextMenu.y"
						@close="contextMenu = null"
						@edit="onEdit"
						@archive="onArchive"
						@delete="onDelete"
					/>

					<button
						type="button"
						class="fixed bottom-20 right-4 z-20 flex h-12 w-12 items-center justify-center rounded-full bg-brand text-white shadow-tmgr-md transition-transform hover:scale-105 hover:bg-brand-hover focus:outline-none focus:ring-2 focus:ring-brand/40 md:bottom-6 md:right-6 md:h-14 md:w-14"
						title="New routine (N)"
						aria-label="New routine"
						@click="onNewRoutine"
					>
						<Plus :size="22" stroke-width="2.4" />
					</button>
				</div>
			</template>
		</BaseLayout>
	</FeatureGate>
</template>

<script setup lang="ts">
	import { computed, onMounted, onBeforeUnmount, ref, watch } from 'vue';
	import { useStore } from 'vuex';
	import BaseLayout from '@/components/layouts/BaseLayout.vue';
	import FeatureGate from '@/components/general/FeatureGate.vue';
	import DailyRoutinesPreview from '@/components/previews/DailyRoutinesPreview.vue';
	import { SkeletonListItem } from '@/components/ui/skeleton';
	import { CalendarCheck, Plus, Upload, Download } from 'lucide-vue-next';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { useDailyRoutineViewport } from '@/composable/useDailyRoutineViewport';
	import { useRoutineDrag } from '@/composable/useRoutineDrag';
	import {
		addDays,
		fmtDate,
		monthShort,
		startOfMonth,
		startOfWeek,
	} from '@/utils/dailyRoutines/dateHelpers';
	import {
		quickCreateRoutine,
		updateDailyTask,
		getDailyTask,
	} from '@/actions/tmgr/daily-tasks';
	import ViewSwitcher from '@/components/dailyRoutine/ViewSwitcher.vue';
	import DRIcon from '@/components/dailyRoutine/DRIcon.vue';
	import CountChip from '@/components/dailyRoutine/CountChip.vue';
	import EditRoutineModal from '@/components/dailyRoutine/EditRoutineModal.vue';
	import RoutineContextMenu from '@/components/dailyRoutine/RoutineContextMenu.vue';
	import RoutineRow from '@/components/dailyRoutine/RoutineRow.vue';
	import ListView from '@/components/dailyRoutine/views/ListView.vue';
	import DayView from '@/components/dailyRoutine/views/DayView.vue';
	import WeekView from '@/components/dailyRoutine/views/WeekView.vue';
	import MonthView from '@/components/dailyRoutine/views/MonthView.vue';
	import YearView from '@/components/dailyRoutine/views/YearView.vue';
	import type { RoutineEntry, ViewId } from '@/types/dailyRoutine';

	const store = useStore();
	const rootRef = ref<HTMLElement | null>(null);
	const fileRef = ref<HTMLInputElement | null>(null);
	const { width, isMobile, isTablet, bind } = useDailyRoutineViewport();

	const VIEW_STORAGE_KEY = 'tmgr.dailyRoutines.view';
	const ALLOWED_VIEWS: ViewId[] = ['list', 'day', 'week', 'month', 'year'];
	function loadStoredView(): ViewId {
		try {
			const v = localStorage.getItem(VIEW_STORAGE_KEY);
			if (v && (ALLOWED_VIEWS as string[]).includes(v)) return v as ViewId;
		} catch {}
		return 'list';
	}
	const view = ref<ViewId>(loadStoredView());
	watch(view, v => {
		try { localStorage.setItem(VIEW_STORAGE_KEY, v); } catch {}
	});
	const cursor = ref<Date>(new Date());
	const editingRoutine = ref<any | null>(null);

	const { setDropHandler, setEditHandler, active: dragActiveRef, hoverKey: dragHoverKey } = useRoutineDrag();
	const dragActive = computed(() => !!dragActiveRef.value);
	setDropHandler(async (entry, payload) => {
		await onMoveRoutine({
			entry,
			date: payload.date,
			timeH: payload.timeH,
			timeM: payload.timeM,
			allDay: payload.allDay,
		});
	});
	setEditHandler(entry => {
		onEdit(entry as RoutineEntry);
	});

	const contextMenu = ref<{ entry: RoutineEntry; x: number; y: number } | null>(null);
	function onContext(payload: { entry: RoutineEntry; x: number; y: number }) {
		contextMenu.value = payload;
	}

	const entries = computed<RoutineEntry[]>(() => store.state.dailyRoutines.entries);
	const yearStats = computed(() => store.state.dailyRoutines.yearStats);
	const isLoading = computed(() => store.state.dailyRoutines.isLoading);

	const todayIso = computed(() => fmtDate(new Date()));
	const todayEntries = computed(() => entries.value.filter(e => e.date === todayIso.value));
	const dayEntries = computed(() => entries.value.filter(e => e.date === fmtDate(cursor.value)));
	const weekStart = computed(() => startOfWeek(cursor.value));
	const unscheduledEntries = computed(() =>
		entries.value.filter(e => !e.time && e.date === todayIso.value),
	);
	const unscheduledDoneCount = computed(
		() => unscheduledEntries.value.filter(e => e.completed).length,
	);
	const scheduledEntries = computed(() =>
		entries.value.filter(e => !!e.time),
	);

	const counts = computed(() => {
		const todays = todayEntries.value;
		return {
			routines: todays.length,
			completed: todays.filter(e => e.completed).length,
			archived: 0,
		};
	});

	const dateLabel = computed(() => {
		const c = cursor.value;
		if (view.value === 'day') {
			return c.toLocaleDateString('en-US', {
				weekday: 'short',
				month: 'long',
				day: 'numeric',
				year: 'numeric',
			});
		}
		if (view.value === 'week') {
			const ws = startOfWeek(c);
			const we = addDays(ws, 6);
			const sameMonth = ws.getMonth() === we.getMonth();
			return sameMonth
				? `${monthShort(ws)} ${ws.getDate()} – ${we.getDate()}, ${ws.getFullYear()}`
				: `${monthShort(ws)} ${ws.getDate()} – ${monthShort(we)} ${we.getDate()}, ${ws.getFullYear()}`;
		}
		if (view.value === 'month') {
			return `${c.toLocaleDateString('en-US', { month: 'long' })} ${c.getFullYear()}`;
		}
		if (view.value === 'year') return String(c.getFullYear());
		return '';
	});

	function step(dir: number) {
		const d = new Date(cursor.value);
		if (view.value === 'day') d.setDate(d.getDate() + dir);
		if (view.value === 'week') d.setDate(d.getDate() + 7 * dir);
		if (view.value === 'month') d.setMonth(d.getMonth() + dir);
		if (view.value === 'year') d.setFullYear(d.getFullYear() + dir);
		cursor.value = d;
	}

	function goToday() {
		cursor.value = new Date();
	}

	function rangeForView(): { from: string; to: string } {
		const c = cursor.value;
		if (view.value === 'list' || view.value === 'day') {
			const iso = fmtDate(c);
			return { from: iso, to: iso };
		}
		if (view.value === 'week') {
			const s = startOfWeek(c);
			return { from: fmtDate(s), to: fmtDate(addDays(s, 6)) };
		}
		if (view.value === 'month') {
			const s = startOfWeek(startOfMonth(c));
			return { from: fmtDate(s), to: fmtDate(addDays(s, 41)) };
		}
		// year — handled separately via stats
		return { from: fmtDate(c), to: fmtDate(c) };
	}

	async function reload() {
		if (view.value === 'year') {
			await Promise.all([
				store.dispatch('dailyRoutines/loadYearStats', cursor.value.getFullYear()),
				store.dispatch('dailyRoutines/loadRange', { from: todayIso.value, to: todayIso.value }),
			]);
		} else {
			await store.dispatch('dailyRoutines/loadRange', rangeForView());
		}
	}

	watch([view, cursor], reload);

	onMounted(() => {
		setDocumentTitle('Daily Routines');
		bind(rootRef.value);
		reload();
		window.addEventListener('keydown', onShortcutKey);
	});

	onBeforeUnmount(() => {
		window.removeEventListener('keydown', onShortcutKey);
	});

	function onNewRoutine() {
		const now = new Date();
		const h = now.getHours();
		const m = now.getMinutes() < 30 ? 30 : 0;
		const hours = m === 0 ? (h + 1) % 24 : h;
		editingRoutine.value = {
			id: null,
			title: '',
			description: '',
			time: { h: hours, m },
			frequency: 'NONE',
			_draftDate: fmtDate(cursor.value),
			recurrence: {
				time: { hours, minutes: m },
				duration_min: 30,
			},
		};
	}

	function onShortcutKey(e: KeyboardEvent) {
		if (e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey) return;
		const t = e.target as HTMLElement | null;
		if (t && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName))) return;
		if (editingRoutine.value) return;
		if (e.key === 'n' || e.key === 'N') {
			e.preventDefault();
			onNewRoutine();
		}
	}

	async function onToggle(entry: RoutineEntry) {
		await store.dispatch('dailyRoutines/toggleComplete', entry);
	}

	async function onEdit(entry: RoutineEntry) {
		try {
			const full = await getDailyTask(entry.task_id);
			editingRoutine.value = full;
		} catch {
			editingRoutine.value = entry;
		}
	}

	async function onDelete(entry: RoutineEntry) {
		await store.dispatch('dailyRoutines/deleteRoutine', entry.task_id);
		await reload();
	}

	async function onArchive(entry: RoutineEntry) {
		await store.dispatch('dailyRoutines/archiveRoutine', entry.task_id);
		await reload();
	}

	async function onArchiveDoneUnscheduled() {
		await store.dispatch('dailyRoutines/archiveDoneUnscheduled');
		await reload();
	}

	function onCreateAt(payload: { date: string; timeH: number; timeM: number }) {
		editingRoutine.value = {
			id: null,
			title: '',
			description: '',
			time: { h: payload.timeH, m: payload.timeM },
			frequency: 'NONE',
			_draftDate: payload.date,
			recurrence: {
				time: { hours: payload.timeH, minutes: payload.timeM },
				duration_min: 30,
			},
		};
	}

	function onSelectDay(d: Date) {
		cursor.value = d;
		view.value = 'day';
	}

	async function onSaveRoutine(draft: any) {
		const dowKeys = ['MON', 'TUE', 'WED', 'THU', 'FRI', 'SAT', 'SUN'];
		const time = `${String(draft.timeH).padStart(2, '0')}:${String(draft.timeM).padStart(2, '0')}`;
		const isRecurring = draft.frequency !== 'NONE';
		const isUnscheduled = !isRecurring && !!draft.unscheduled;
		const payload: any = {
			title: draft.title,
			description: draft.description,
			routine_category: draft.cat,
			is_daily_routine: true,
			is_recurring: isRecurring,
		};
		if (isRecurring) {
			payload.recurrence = {
				frequency: draft.frequency,
				interval: draft.interval || 1,
				days_of_week: draft.frequency === 'WEEKLY' ? draft.daysOfWeek.map((i: number) => dowKeys[i]) : null,
				day_of_frequency: ['MONTHLY', 'YEARLY'].includes(draft.frequency) ? draft.dayOfMonth : null,
				month: draft.frequency === 'YEARLY' ? draft.month : null,
				time: { hours: draft.timeH, minutes: draft.timeM },
				duration_min: draft.durationMin,
				reminder_min: draft.reminderMin,
			};
		} else if (isUnscheduled) {
			payload.scheduled_date = null;
			payload.scheduled_time = null;
		} else {
			payload.scheduled_date = draft.scheduledDate || todayIso.value;
			payload.scheduled_time = { hours: draft.timeH, minutes: draft.timeM };
		}
		if (draft.id) {
			await updateDailyTask(draft.id, payload);
		} else {
			const draftDate = isUnscheduled ? undefined : (draft.scheduledDate || editingRoutine.value?._draftDate);
			const created = await quickCreateRoutine({
				title: draft.title,
				date: draftDate,
				time: isUnscheduled ? undefined : time,
				category: draft.cat,
			});
			const newId = created?.id ?? created?.task_id;
			if (newId && (isRecurring || draft.description || isUnscheduled)) {
				await updateDailyTask(newId, payload);
			}
		}
		editingRoutine.value = null;
		await reload();
	}

	async function onMoveRoutine(payload: { entry: RoutineEntry; date: string; timeH?: number; timeM?: number; allDay?: boolean }) {
		await store.dispatch('dailyRoutines/moveRoutine', payload);
		await reload();
	}

	async function onDeleteRoutine(routine: any) {
		await store.dispatch('dailyRoutines/deleteRoutine', routine.id);
		editingRoutine.value = null;
		await reload();
	}

	function triggerImport() {
		fileRef.value?.click();
	}
	async function onImportFile(e: Event) {
		const input = e.target as HTMLInputElement;
		const file = input.files?.[0];
		if (!file) return;
		const result = await store.dispatch('dailyRoutines/importIcs', { file, mode: 'skip' });
		input.value = '';
		const errs = Array.isArray(result.errors) && result.errors.length
			? `\nErrors:\n- ${result.errors.slice(0, 5).join('\n- ')}`
			: '';
		// eslint-disable-next-line no-alert
		alert(`Imported ${result.created}, skipped ${result.skipped}, replaced ${result.replaced}.${errs}`);
		await reload();
	}
	async function onExport() {
		await store.dispatch('dailyRoutines/exportIcs');
	}
</script>
