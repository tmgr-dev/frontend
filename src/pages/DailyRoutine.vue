<template>
	<FeatureGate
		feature-key="daily_routines"
		:title="$t('dailyRoutine.page.gate.title')"
		:description="$t('dailyRoutine.page.gate.description')"
		:icon="CalendarCheck"
	>
		<template #preview>
			<DailyRoutinesPreview />
		</template>

		<BaseLayout :title="pageTitle">
			<template #body>
				<div
					ref="rootRef"
					class="relative flex h-full flex-col overflow-hidden pb-10"
				>
					<div
						v-if="isLocalWorkspace"
						class="flex flex-wrap items-center gap-2 border-b border-line pt-3 pb-2 md:pt-4"
					>
						<span class="text-sm font-semibold text-ink">{{
							$t('dailyRoutine.page.localRoutines')
						}}</span>
						<span
							class="shrink-0 rounded bg-amber-500/15 px-1.5 py-0.5 text-2xs font-semibold text-amber-600 uppercase dark:text-amber-400"
							>{{ $t('dailyRoutine.page.localBadge') }}</span
						>
						<span
							data-testid="local-routines-label"
							class="w-full text-xs text-ink-subtle md:w-auto"
						>
							{{ $t('dailyRoutine.page.storedLocally') }}
						</span>
						<button
							v-if="cloudTargets.length"
							type="button"
							class="rounded-md border px-2.5 py-1 text-xs font-medium transition-colors md:ml-auto"
							:class="
								selectMode
									? 'border-brand bg-brand-bg text-brand'
									: 'border-line text-ink hover:bg-surface-hover'
							"
							:aria-pressed="selectMode"
							data-testid="local-routines-select"
							@click="selectMode = !selectMode"
						>
							{{ $t('dailyRoutine.page.select') }}
						</button>
						<span
							v-else
							class="text-xs text-ink-subtle md:ml-auto"
							data-testid="local-routines-move-note"
						>
							{{ moveNote }}
						</span>
					</div>

					<!-- Top toolbar -->
					<div
						class="flex flex-col gap-3 border-b border-line pt-3 pb-3 md:pt-4"
					>
						<!-- Row 1: Quick-add (full width on mobile; chips + import/export inline on desktop) -->
						<div class="flex flex-wrap items-center gap-2.5">
							<div class="flex min-w-0 flex-1 flex-col gap-1">
								<div
									class="flex items-center gap-2 rounded-card border bg-surface px-3 py-1.5 shadow-tmgr-xs transition-colors"
									:class="quickFocused ? 'border-line-focus' : 'border-line'"
								>
									<Inbox
										:size="16"
										stroke-width="2"
										class="shrink-0 text-ink-subtle"
									/>
									<input
										ref="quickInputRef"
										v-model="quickTitle"
										:aria-label="$t('dailyRoutine.page.quickAddAria')"
										:placeholder="
											isMobile
												? $t('dailyRoutine.page.quickAddMobile')
												: $t('dailyRoutine.page.quickAddPlaceholder')
										"
										class="min-w-0 flex-1 bg-transparent text-sm text-ink outline-hidden placeholder:text-ink-subtle"
										data-tour="routines.quick-add"
										@focus="quickFocused = true"
										@blur="quickFocused = false"
										@keyup.enter="onQuickAdd"
										@keydown.esc="onQuickEsc"
									/>
									<kbd
										v-if="!isMobile && !quickTitle"
										class="hidden shrink-0 rounded border border-line bg-surface px-1.5 py-0.5 font-mono text-[10px] text-ink-subtle md:inline-flex"
										>/</kbd
									>
									<button
										type="button"
										class="flex h-8 shrink-0 items-center gap-1 rounded-pill bg-brand px-3 text-2xs font-semibold text-on-accent shadow-tmgr-xs transition-colors hover:bg-brand-hover disabled:cursor-not-allowed disabled:opacity-50"
										:disabled="!quickTitle.trim()"
										@click="onQuickAdd"
									>
										<span>{{ $t('common.actions.add') }}</span>
										<kbd
											class="rounded border border-white/30 px-1 font-mono text-[10px] leading-none"
											>↵</kbd
										>
									</button>
								</div>
								<div
									v-if="!isMobile"
									class="pl-1 text-[10px] text-ink-subtle transition-opacity"
									:class="quickFocused ? 'opacity-100' : 'opacity-0'"
									aria-hidden="true"
								>
									<i18n-t keypath="dailyRoutine.page.quickAddHint" tag="span">
										<template #unscheduled>
											<span class="font-semibold text-ink">{{
												$t('dailyRoutine.unscheduled')
											}}</span>
										</template>
									</i18n-t>
								</div>
							</div>
							<input
								v-if="!isLocalWorkspace"
								ref="fileRef"
								type="file"
								accept=".ics,text/calendar"
								class="hidden"
								@change="onImportFile"
							/>
						</div>

						<!-- Mobile only: stats chip strip (horizontal scroll) -->
						<div
							v-if="isMobile"
							class="flex shrink-0 [scrollbar-width:none] gap-1.5 overflow-x-auto pb-0.5 [-ms-overflow-style:none]"
						>
							<div
								class="flex shrink-0 items-baseline gap-1.5 rounded-pill border border-line bg-surface px-2.5 py-1"
							>
								<span class="text-sm font-bold text-ink tabular-nums">{{
									counts.routines
								}}</span>
								<span
									class="text-[10px] font-semibold tracking-wider text-ink-subtle uppercase"
									>{{ $t('dailyRoutine.page.countRoutines') }}</span
								>
							</div>
							<div
								class="flex shrink-0 items-baseline gap-1.5 rounded-pill border border-line bg-surface px-2.5 py-1"
							>
								<span
									class="text-sm font-bold tabular-nums"
									style="color: #22c55e"
									>{{ counts.completed }}</span
								>
								<span
									class="text-[10px] font-semibold tracking-wider text-ink-subtle uppercase"
									>{{ $t('dailyRoutine.page.countDone') }}</span
								>
							</div>
							<div
								class="flex shrink-0 items-baseline gap-1.5 rounded-pill border border-line bg-surface px-2.5 py-1"
							>
								<span class="text-sm font-bold text-ink tabular-nums">{{
									counts.archived
								}}</span>
								<span
									class="text-[10px] font-semibold tracking-wider text-ink-subtle uppercase"
									>{{ $t('dailyRoutine.page.countArchived') }}</span
								>
							</div>
						</div>

						<!-- Row: View switcher + chips/Import/Export (desktop) or Import/Export icons (mobile) + Today/nav/date when not list -->
						<div
							class="flex flex-wrap items-center gap-3 overflow-x-auto pb-1 sm:flex-nowrap"
						>
							<ViewSwitcher v-model="view" :compact="isTablet" />
							<template v-if="!isMobile">
								<div class="flex-1" />
								<div class="flex shrink-0 gap-1.5">
									<CountChip
										:n="counts.routines"
										:label="$t('dailyRoutine.page.chipRoutines')"
									/>
									<CountChip
										:n="counts.completed"
										:label="$t('dailyRoutine.page.chipDone')"
										:color="'#22c55e'"
									/>
									<CountChip
										:n="counts.archived"
										:label="$t('dailyRoutine.page.chipArchived')"
									/>
								</div>
								<template v-if="!isLocalWorkspace">
									<button
										v-if="isFeatureEnabled('import')"
										type="button"
										class="flex h-8 shrink-0 items-center gap-1.5 rounded-pill border border-line bg-surface px-3 text-2xs font-medium text-ink-subtle transition-colors hover:border-brand hover:text-brand"
										:title="$t('dailyRoutine.page.importCalendar')"
										@click="triggerImport"
									>
										<Upload :size="14" stroke-width="2" />
										<span>{{ $t('dailyRoutine.page.import') }}</span>
									</button>
									<button
										type="button"
										class="flex h-8 shrink-0 items-center gap-1.5 rounded-pill border border-line bg-surface px-3 text-2xs font-medium text-ink-subtle transition-colors hover:border-brand hover:text-brand"
										:title="$t('dailyRoutine.page.exportCalendar')"
										@click="onExport"
									>
										<Download :size="14" stroke-width="2" />
										<span>{{ $t('dailyRoutine.page.export') }}</span>
									</button>
								</template>
							</template>
							<template v-if="isMobile && !isLocalWorkspace">
								<button
									v-if="isFeatureEnabled('import')"
									type="button"
									class="flex h-8 w-8 shrink-0 items-center justify-center rounded-pill border border-line bg-surface text-ink-subtle hover:text-brand"
									:title="$t('dailyRoutine.page.importIcs')"
									@click="triggerImport"
								>
									<Upload :size="14" stroke-width="2" />
								</button>
								<button
									type="button"
									class="flex h-8 w-8 shrink-0 items-center justify-center rounded-pill border border-line bg-surface text-ink-subtle hover:text-brand"
									:title="$t('dailyRoutine.page.exportIcs')"
									@click="onExport"
								>
									<Download :size="14" stroke-width="2" />
								</button>
							</template>
							<template v-if="view !== 'list'">
								<button
									type="button"
									class="shrink-0 rounded-card border border-line bg-transparent px-3 py-1.5 text-2xs font-medium text-ink hover:bg-surface-hover"
									@click="goToday"
								>
									{{ $t('dailyRoutine.today') }}
								</button>
								<div class="flex shrink-0 gap-0.5">
									<button
										type="button"
										:aria-label="$t('dailyRoutine.page.prevPeriod')"
										class="flex h-7 w-7 items-center justify-center rounded-md text-ink-subtle hover:bg-surface-hover hover:text-ink"
										@click="step(-1)"
									>
										<DRIcon name="chev-l" :size="13" stroke="currentColor" />
									</button>
									<button
										type="button"
										:aria-label="$t('dailyRoutine.page.nextPeriod')"
										class="flex h-7 w-7 items-center justify-center rounded-md text-ink-subtle hover:bg-surface-hover hover:text-ink"
										@click="step(1)"
									>
										<DRIcon name="chev-r" :size="13" stroke="currentColor" />
									</button>
								</div>
								<span
									class="ml-1 text-sm font-semibold whitespace-nowrap text-ink"
									>{{ dateLabel }}</span
								>
							</template>
						</div>
					</div>

					<!-- Body -->
					<div class="flex flex-1 flex-col gap-3 overflow-hidden p-2 md:p-5">
						<MoveRoutinesPicker
							v-if="isLocalWorkspace && selectMode"
							:targets="cloudTargets"
							@cancel="selectMode = false"
							@changed="reload"
						/>
						<AsyncContent
							v-else
							class="flex min-h-0 flex-1 flex-col gap-3"
							:pending="viewPending"
							:loaded="viewLoaded"
							:error="viewError"
							:retry="reload"
							:label="$t('dailyRoutine.page.loading')"
						>
							<template #skeleton
								><RoutineViewSkeleton :view="view"
							/></template>
							<!-- Unscheduled section (visible above all calendar views) -->
							<div
								v-if="
									view !== 'list' &&
									view !== 'day' &&
									(unscheduledEntries.length || dragActive)
								"
								class="flex max-h-[220px] shrink-0 flex-col gap-1.5 overflow-y-auto rounded-card transition-colors"
								:class="
									dragHoverKey === `unsched:${todayIso}`
										? 'bg-brand/10 ring-1 ring-brand/40'
										: ''
								"
								data-dr-drop
								data-dr-kind="unscheduled"
								:data-dr-date="todayIso"
							>
								<div class="flex items-center justify-between gap-2 px-1">
									<div
										class="text-[10px] font-bold tracking-wider text-ink-subtle uppercase"
									>
										{{ $t('dailyRoutine.unscheduled') }}
										<span>{{ unscheduledEntries.length }}</span>
									</div>
									<button
										v-if="doneUnscheduledIdList.length > 0"
										type="button"
										class="rounded-pill border border-line bg-surface px-2.5 py-1 text-[10px] font-semibold tracking-wider text-ink-subtle uppercase hover:border-brand hover:text-brand"
										@click="onArchiveDoneUnscheduled(doneUnscheduledIdList)"
									>
										{{
											$t('dailyRoutine.archiveDone', {
												n: doneUnscheduledIdList.length,
											})
										}}
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
								@resize="onResizeRoutine"
							/>
							<WeekView
								v-else-if="view === 'week'"
								:entries="scheduledEntries"
								:week-start="weekStart"
								:is-mobile="isMobile"
								:is-tablet="isTablet"
								@select-day="onSelectDay"
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
						</AsyncContent>
					</div>

					<EditRoutineModal
						v-if="editingRoutine"
						:routine="editingRoutine"
						:busy="editorBusy"
						:error="editorError"
						:confirm-open="deleteDialogOpen"
						:comments-enabled="!isLocalWorkspace"
						@comments-count="onCommentsCount"
						@close="editingRoutine = null"
						@save="onSaveRoutine"
						@delete="onDeleteRoutine"
						@convert="onConvertRoutine"
					/>

					<DeleteRoutineDialog
						v-model:open="deleteDialogOpen"
						:title="deleteTarget?.title ?? ''"
						:recurring="deleteTarget?.recurring ?? false"
						:comments-count="deleteTarget?.commentsCount ?? 0"
						@confirm="confirmDelete"
					/>

					<RoutineScopeDialog
						v-model:open="scopeDialogOpen"
						:recurrence-changed="scopeRecurrenceChanged"
						@select="onScopeSelected"
					/>

					<AlertDialog
						:open="showConvertedTaskPrompt"
						@update:open="(open) => (showConvertedTaskPrompt = open)"
					>
						<AlertDialogContent>
							<AlertDialogHeader>
								<AlertDialogTitle>{{
									$t('dailyRoutine.page.converted.title')
								}}</AlertDialogTitle>
								<AlertDialogDescription>
									{{ $t('dailyRoutine.page.converted.description') }}
								</AlertDialogDescription>
							</AlertDialogHeader>
							<AlertDialogFooter>
								<AlertDialogCancel>{{
									$t('common.states.no')
								}}</AlertDialogCancel>
								<AlertDialogAction @click="openConvertedTask">
									{{ $t('common.states.yes') }}
								</AlertDialogAction>
							</AlertDialogFooter>
						</AlertDialogContent>
					</AlertDialog>

					<RoutineContextMenu
						v-if="contextMenu"
						:entry="contextMenu.entry"
						:x="contextMenu.x"
						:y="contextMenu.y"
						@close="contextMenu = null"
						@edit="onEdit"
						@archive="onArchive"
						@delete="onDelete"
						@duplicate="onDuplicate"
						@convert="convertingEntry = $event"
					/>

					<ConvertRoutineModal
						v-if="convertingEntry"
						:entry="convertingEntry"
						:busy="convertBusy"
						@close="convertingEntry = null"
						@convert="onConvertFromMenu"
					/>

					<button
						v-if="!selectMode"
						type="button"
						class="fixed right-4 bottom-20 z-20 flex h-12 w-12 items-center justify-center rounded-full bg-brand text-on-accent shadow-tmgr-md transition-transform hover:scale-105 hover:bg-brand-hover md:right-6 md:bottom-[calc(var(--statusbar-h,0px)+1.5rem)] md:h-14 md:w-14"
						:title="$t('dailyRoutine.page.newRoutineTitle')"
						:aria-label="$t('dailyRoutine.page.newRoutine')"
						data-tour="routines.add"
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
	import {
		expandRoutineRange,
		getDailyTask,
		quickCreateRoutine,
		updateDailyTask,
	} from '@/actions/tmgr/daily-tasks';
	import { updateTask } from '@/actions/tmgr/tasks';
	import { getWorkspaces, type Workspace } from '@/actions/tmgr/workspaces';
	import AsyncContent from '@/components/async/AsyncContent.vue';
	import ConvertRoutineModal from '@/components/dailyRoutine/ConvertRoutineModal.vue';
	import CountChip from '@/components/dailyRoutine/CountChip.vue';
	import DRIcon from '@/components/dailyRoutine/DRIcon.vue';
	import DeleteRoutineDialog from '@/components/dailyRoutine/DeleteRoutineDialog.vue';
	import EditRoutineModal from '@/components/dailyRoutine/EditRoutineModal.vue';
	import RoutineContextMenu from '@/components/dailyRoutine/RoutineContextMenu.vue';
	import RoutineScopeDialog from '@/components/dailyRoutine/RoutineScopeDialog.vue';
	import {
		buildRoutinePayload,
		isStaleSeriesError,
		needsScopeChoice,
		newOpId,
		recurrenceChanged,
		type RoutineScope,
		occurrenceRangeOf,
		routineDraftFrom,
		routineForEdit,
		type RoutineDraft,
	} from '@/components/dailyRoutine/routinePayload';
	import MoveRoutinesPicker from '@/components/dailyRoutine/moveToCloud/MoveRoutinesPicker.vue';
	import RoutineRow from '@/components/dailyRoutine/RoutineRow.vue';
	import RoutineViewSkeleton from '@/components/dailyRoutine/RoutineViewSkeleton.vue';
	import DayView from '@/components/dailyRoutine/views/DayView.vue';
	import ListView from '@/components/dailyRoutine/views/ListView.vue';
	import MonthView from '@/components/dailyRoutine/views/MonthView.vue';
	import WeekView from '@/components/dailyRoutine/views/WeekView.vue';
	import YearView from '@/components/dailyRoutine/views/YearView.vue';
	import ViewSwitcher from '@/components/dailyRoutine/ViewSwitcher.vue';
	import FeatureGate from '@/components/general/FeatureGate.vue';
	import BaseLayout from '@/components/layouts/BaseLayout.vue';
	import DailyRoutinesPreview from '@/components/previews/DailyRoutinesPreview.vue';
	import {
		AlertDialog,
		AlertDialogAction,
		AlertDialogCancel,
		AlertDialogContent,
		AlertDialogDescription,
		AlertDialogFooter,
		AlertDialogHeader,
		AlertDialogTitle,
	} from '@/components/ui/alert-dialog';
	import { useToast } from '@/components/ui/toast';
	import { useDailyRoutineViewport } from '@/composable/useDailyRoutineViewport';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { useFeatureToggles } from '@/composable/useFeatureToggles';
	import { useRoutineDrag } from '@/composable/useRoutineDrag';
	import type { RoutineEntry, ViewId } from '@/types/dailyRoutine';
	import { currentLocale, t } from '@/i18n';
	import {
		addDays,
		fmtDate,
		startOfMonth,
		startOfWeek,
	} from '@/utils/dailyRoutines/dateHelpers';
	import { getArchivedDailyTasksCount } from '@/actions/tmgr/daily-tasks';
	import { doneUnscheduledIds } from '@/utils/dailyRoutines/archiveDone';
	import { sortUnscheduledNewestFirst } from '@/utils/dailyRoutines/sortRoutines';
	import { requestCache } from '@/utils/requestCache';
	import { nowDateIn, todayDateIn, todayIn } from '@/utils/timezone';
	import {
		CalendarCheck,
		Download,
		Inbox,
		Plus,
		Upload,
	} from 'lucide-vue-next';
	import {
		computed,
		nextTick,
		onBeforeUnmount,
		onMounted,
		ref,
		watch,
		type Ref,
	} from 'vue';
	import { useStore } from 'vuex';

	const store = useStore();
	const { toast } = useToast();
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
	watch(view, (v) => {
		try {
			localStorage.setItem(VIEW_STORAGE_KEY, v);
		} catch {}
	});
	const cursor = ref<Date>(todayDateIn(store.getters.userTimeZone));
	const editingRoutine = ref<any | null>(null);
	const editorBusy = ref(false);
	const editorError = ref('');
	// Set once quick-create succeeded, so a retry after a failed follow-up PUT
	// updates that row instead of creating a duplicate.
	const createdRoutineId = ref<number | null>(null);
	watch(editingRoutine, () => {
		editorError.value = '';
		createdRoutineId.value = null;
	});
	const deleteDialogOpen = ref(false);
	const deleteTarget = ref<{
		taskId: number;
		title: string;
		fromEditor: boolean;
		recurring: boolean;
		occurrenceDate: string;
		commentsCount: number;
	} | null>(null);
	const editingEntry = ref<RoutineEntry | null>(null);
	const scopeDialogOpen = ref(false);
	const scopeRecurrenceChanged = ref(false);
	const pendingDraft = ref<RoutineDraft | null>(null);
	const quickTitle = ref('');
	const quickFocused = ref(false);
	const quickInputRef = ref<HTMLInputElement | null>(null);

	const {
		setDropHandler,
		setEditHandler,
		active: dragActiveRef,
		hoverKey: dragHoverKey,
	} = useRoutineDrag();
	const dragActive = computed(() => !!dragActiveRef.value);
	setDropHandler(async (entry, payload) => {
		await onMoveRoutine({
			entry,
			date: payload.date,
			timeH: payload.timeH,
			timeM: payload.timeM,
			allDay: payload.allDay,
			unscheduled: payload.unscheduled,
		});
	});
	setEditHandler((entry) => {
		onEdit(entry as RoutineEntry);
	});

	const contextMenu = ref<{ entry: RoutineEntry; x: number; y: number } | null>(
		null,
	);
	function onContext(payload: { entry: RoutineEntry; x: number; y: number }) {
		contextMenu.value = payload;
	}

	const entries = computed<RoutineEntry[]>(
		() => store.state.dailyRoutines.entries,
	);
	const yearStats = computed(() => store.state.dailyRoutines.yearStats);

	const todayIso = computed(() => todayIn(store.getters.userTimeZone));
	const todayEntries = computed(() =>
		entries.value.filter((e) => e.date === todayIso.value),
	);
	const dayEntries = computed(() =>
		entries.value.filter((e) => e.date === fmtDate(cursor.value)),
	);
	const weekStart = computed(() => startOfWeek(cursor.value));
	const unscheduledEntries = computed(() =>
		sortUnscheduledNewestFirst(
			entries.value.filter((e) => !e.time && e.date === todayIso.value),
		),
	);
	const doneUnscheduledIdList = computed(() =>
		doneUnscheduledIds(unscheduledEntries.value),
	);
	const archivedCount = ref(0);
	async function refreshArchivedCount() {
		try {
			archivedCount.value = Number(await getArchivedDailyTasksCount());
		} catch {
			archivedCount.value = 0;
		}
	}
	const scheduledEntries = computed(() =>
		entries.value.filter((e) => !!e.time),
	);

	const counts = computed(() => {
		const todays = todayEntries.value;
		return {
			routines: todays.length,
			completed: todays.filter((e) => e.completed).length,
			archived: archivedCount.value,
		};
	});

	const dateLabel = computed(() => {
		const c = cursor.value;
		if (view.value === 'day') {
			return c.toLocaleDateString(currentLocale(), {
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
			const shortMonth = (d: Date) =>
				d.toLocaleDateString(currentLocale(), { month: 'short' });
			return sameMonth
				? t('dailyRoutine.page.weekRange', {
						month: shortMonth(ws),
						start: ws.getDate(),
						end: we.getDate(),
						year: ws.getFullYear(),
					})
				: t('dailyRoutine.page.weekRangeCross', {
						startMonth: shortMonth(ws),
						start: ws.getDate(),
						endMonth: shortMonth(we),
						end: we.getDate(),
						year: ws.getFullYear(),
					});
		}
		if (view.value === 'month') {
			return t('dailyRoutine.page.monthLabel', {
				month: c.toLocaleDateString(currentLocale(), { month: 'long' }),
				year: c.getFullYear(),
			});
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
		cursor.value = todayDateIn(store.getters.userTimeZone);
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

	const viewPending = ref(true),
		viewLoaded = ref(false),
		viewError = ref<string | null>(null);
	let viewRequest = 0;
	const workspaceIdentity = computed(
		() =>
			store.state.user?.settings?.find(
				(s: { key: string; value: unknown }) => s.key === 'current_workspace',
			)?.value,
	);
	// Last shown routines, so a return visit renders them while reload() refreshes (TM-703).
	interface RoutinesSnapshot {
		entries: RoutineEntry[];
		yearStats?: Record<string, unknown>;
	}
	function entriesRange(): { from: string; to: string } {
		return view.value === 'year'
			? { from: todayIso.value, to: todayIso.value }
			: rangeForView();
	}
	function routinesSnapshotKey(): string | null {
		if (workspaceIdentity.value == null) return null;
		return requestCache.generateKey(
			'routines-snapshot',
			view.value === 'year'
				? {
						workspace: workspaceIdentity.value,
						year: cursor.value.getFullYear(),
						today: todayIso.value,
					}
				: { workspace: workspaceIdentity.value, ...rangeForView() },
		);
	}
	function saveRoutinesSnapshot(key = routinesSnapshotKey()) {
		const { entries, yearStats, lastRange } = store.state.dailyRoutines;
		const range = entriesRange();
		if (
			!key ||
			key !== routinesSnapshotKey() ||
			lastRange.from !== range.from ||
			lastRange.to !== range.to
		)
			return;
		requestCache.set<RoutinesSnapshot>(
			key,
			view.value === 'year'
				? { entries: [...entries], yearStats: { ...yearStats } }
				: { entries: [...entries] },
		);
	}
	const seededKey = routinesSnapshotKey();
	const routinesSnapshot =
		seededKey && requestCache.get<RoutinesSnapshot>(seededKey);
	if (routinesSnapshot) {
		store.commit('dailyRoutines/setEntries', [...routinesSnapshot.entries]);
		store.commit('dailyRoutines/setLastRange', entriesRange());
		if (routinesSnapshot.yearStats)
			store.commit('dailyRoutines/setYearStats', {
				...routinesSnapshot.yearStats,
			});
		viewLoaded.value = true;
	}
	watch(
		() => store.state.dailyRoutines.entries,
		() => {
			if (viewLoaded.value && !viewError.value) saveRoutinesSnapshot();
		},
	);
	// /routines is workspace-independent, so store.state.workspaces may be empty here.
	const isLocalWorkspace = ref(false);
	const selectMode = ref(false);
	const cloudTargets = ref<Workspace[]>([]);
	const moveNote = computed(() =>
		store.getters.isLoggedIn
			? t('dailyRoutine.page.noCloudWorkspace')
			: t('dailyRoutine.page.signInToMove'),
	);
	const { isFeatureEnabled } = useFeatureToggles();
	const pageTitle = computed(() =>
		isLocalWorkspace.value
			? t('dailyRoutine.page.localRoutines')
			: t('dailyRoutine.page.dailyRoutines'),
	);
	watch(
		workspaceIdentity,
		async (id) => {
			const workspaces = await getWorkspaces().catch(() => []);
			isLocalWorkspace.value = Boolean(
				workspaces.find((w) => Number(w.id) === Number(id))?.is_local,
			);
			cloudTargets.value =
				isLocalWorkspace.value && store.getters.isLoggedIn
					? workspaces.filter((w) => !w.is_local && Number(w.id) > 0)
					: [];
			if (!isLocalWorkspace.value) selectMode.value = false;
		},
		{ immediate: true },
	);
	watch(isLocalWorkspace, (local) => {
		const title = local
			? t('dailyRoutine.page.localRoutines')
			: t('dailyRoutine.page.dailyRoutines');
		store.commit('setMetaTitle', title);
		setDocumentTitle(title);
	});
	async function reload() {
		const request = ++viewRequest;
		const snapshotKey = routinesSnapshotKey();
		viewPending.value = true;
		viewError.value = null;
		void refreshArchivedCount();
		try {
			if (view.value === 'year') {
				await Promise.all([
					store.dispatch(
						'dailyRoutines/loadYearStats',
						cursor.value.getFullYear(),
					),
					store.dispatch('dailyRoutines/loadRange', {
						from: todayIso.value,
						to: todayIso.value,
					}),
				]);
			} else {
				await store.dispatch('dailyRoutines/loadRange', rangeForView());
			}

			if (request === viewRequest) {
				viewLoaded.value = true;
				saveRoutinesSnapshot(snapshotKey);
			}
		} catch (error) {
			if (request === viewRequest)
				viewError.value =
					error instanceof Error
						? error.message
						: t('dailyRoutine.page.loadError');
		} finally {
			if (request === viewRequest) viewPending.value = false;
		}
	}
	watch(workspaceIdentity, () => {
		viewLoaded.value = false;
		void reload();
	});

	watch([view, cursor], () => {
		viewLoaded.value = false;
		void reload();
	});

	onMounted(() => {
		setDocumentTitle(t('dailyRoutine.page.gate.title'));
		bind(rootRef.value);
		reload();
		window.addEventListener('keydown', onShortcutKey);
	});

	onBeforeUnmount(() => {
		++viewRequest;
		window.removeEventListener('keydown', onShortcutKey);
	});

	async function onQuickAdd() {
		const title = quickTitle.value.trim();
		if (!title) return;
		await quickCreateRoutine({ title });
		quickTitle.value = '';
		await reload();
	}

	function onQuickEsc() {
		quickTitle.value = '';
		quickInputRef.value?.blur();
	}

	function onNewRoutine() {
		const now = nowDateIn(store.getters.userTimeZone);
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
		if (
			t &&
			(t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName))
		)
			return;
		if (editingRoutine.value || convertingEntry.value || deleteDialogOpen.value)
			return;
		if (e.key === 'n' || e.key === 'N') {
			e.preventDefault();
			onNewRoutine();
		}
		if (e.key === '/') {
			e.preventDefault();
			quickInputRef.value?.focus();
		}
	}

	async function onToggle(entry: RoutineEntry) {
		const refreshing = viewPending.value;
		await store.dispatch('dailyRoutines/toggleComplete', entry);
		// The in-flight refresh may predate this write and would revert the tick (TM-703).
		if (refreshing || viewPending.value) await reload();
	}

	async function loadRoutineForEdit(entry: RoutineEntry) {
		const recurring = !!entry.frequency && entry.frequency !== 'NONE';
		const full = await getDailyTask(
			entry.task_id,
			recurring && !isLocalWorkspace.value ? entry.date : undefined,
		);
		let occurrences: RoutineEntry[] = [];
		if (entry.frequency === 'WEEKLY' && entry.date) {
			const { from, to } = occurrenceRangeOf(entry.date);
			occurrences = await expandRoutineRange(from, to);
		}
		// A one-off routine's schedule lives on its instance (carried by the
		// entry), not on the task row the GET returns — overlay it so the modal
		// shows the real time/date/duration instead of defaulting to unscheduled.
		return routineForEdit(full, entry, occurrences);
	}

	function errorText(error: unknown): string {
		return (
			(error as any)?.response?.data?.message || t('dailyRoutine.page.tryAgain')
		);
	}

	function onCommentsCount(taskId: number, count: number) {
		const stale = (store.state.dailyRoutines.entries as RoutineEntry[]).some(
			(e) => e.task_id === taskId && e.comments_count !== count,
		);
		if (stale)
			store.commit('dailyRoutines/patchTaskEntries', {
				taskId,
				patch: { comments_count: count },
			});
	}

	async function onEdit(entry: RoutineEntry) {
		try {
			editingRoutine.value = await loadRoutineForEdit(entry);
			editingEntry.value = entry;
		} catch (error) {
			toast({
				title: t('dailyRoutine.page.openError'),
				description: errorText(error),
				variant: 'destructive',
			});
		}
	}

	function onDelete(entry: RoutineEntry) {
		deleteTarget.value = {
			taskId: entry.task_id,
			title: entry.title,
			fromEditor: false,
			recurring:
				!isLocalWorkspace.value &&
				!!entry.frequency &&
				entry.frequency !== 'NONE',
			occurrenceDate: entry.date,
			commentsCount: isLocalWorkspace.value ? 0 : (entry.comments_count ?? 0),
		};
		deleteDialogOpen.value = true;
	}

	async function confirmDelete(scope?: RoutineScope) {
		const target = deleteTarget.value;
		if (!target) return;
		if (target.fromEditor) {
			editorBusy.value = true;
			editorError.value = '';
		}
		try {
			await store.dispatch(
				'dailyRoutines/deleteRoutine',
				target.recurring
					? {
							taskId: target.taskId,
							scope,
							occurrenceDate: target.occurrenceDate,
						}
					: target.taskId,
			);
			if (target.fromEditor) editingRoutine.value = null;
			await reload();
		} catch (error) {
			if (target.fromEditor) {
				editorError.value = errorText(error);
			} else {
				toast({
					title: t('dailyRoutine.page.deleteError'),
					description: errorText(error),
					variant: 'destructive',
				});
			}
		} finally {
			editorBusy.value = false;
		}
	}

	async function onArchive(entry: RoutineEntry) {
		await store.dispatch('dailyRoutines/archiveRoutine', entry.task_id);
		await reload();
	}

	async function onArchiveDoneUnscheduled(ids: number[]) {
		await store.dispatch('dailyRoutines/archiveDoneUnscheduled', ids);
		await reload();
	}

	function onCreateAt(payload: { date: string; timeH: number; timeM: number }) {
		editingEntry.value = null;
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

	async function saveRoutine(
		draft: RoutineDraft,
		fallbackDate?: string,
		created?: Ref<number | null>,
		scope?: RoutineScope,
	) {
		const time = `${String(draft.timeH).padStart(2, '0')}:${String(
			draft.timeM,
		).padStart(2, '0')}`;
		const isUnscheduled = draft.frequency === 'NONE' && !!draft.unscheduled;
		if (draft.id) {
			const original = routineDraftFrom(editingRoutine.value, todayIso.value);
			// The modal raises a shorter duration to its 15 min minimum on open.
			original.durationMin = Math.max(15, original.durationMin);
			const recurring = original.frequency !== 'NONE';
			const versioned = recurring && scope;
			await updateDailyTask(
				draft.id,
				buildRoutinePayload(draft, todayIso.value, original, scope),
				isLocalWorkspace.value || (recurring && !scope)
					? {}
					: {
							scope: scope ?? 'all',
							occurrenceDate: versioned
								? (editingRoutine.value?.scheduled_date ?? todayIso.value)
								: undefined,
							baseVersion: versioned
								? editingRoutine.value?.series_version
								: undefined,
							opId: newOpId(),
						},
			);
		} else {
			const payload = buildRoutinePayload(draft, todayIso.value);
			const draftDate = isUnscheduled
				? undefined
				: draft.scheduledDate || fallbackDate;
			let newId = created?.value ?? null;
			if (!newId) {
				const quick = await quickCreateRoutine({
					title: draft.title,
					date: draftDate,
					time: isUnscheduled ? undefined : time,
					category: draft.cat,
				});
				newId = quick?.id ?? quick?.task_id;
				if (created && newId) created.value = newId;
			}
			// Quick-create only stores title/date/time; always follow up so
			// duration, description and recurrence are persisted too.
			if (newId) {
				await updateDailyTask(
					newId,
					payload,
					isLocalWorkspace.value ? {} : { scope: 'all', opId: newOpId() },
				);
			}
		}
	}

	async function onSaveRoutine(draft: RoutineDraft, scope?: RoutineScope) {
		if (!scope && draft.id) {
			const original = routineDraftFrom(editingRoutine.value, todayIso.value);
			original.durationMin = Math.max(15, original.durationMin);
			if (needsScopeChoice(draft, original, isLocalWorkspace.value)) {
				pendingDraft.value = draft;
				scopeRecurrenceChanged.value = recurrenceChanged(draft, original);
				scopeDialogOpen.value = true;
				return;
			}
		}
		editorBusy.value = true;
		editorError.value = '';
		let saved = false;
		try {
			await saveRoutine(
				draft,
				editingRoutine.value?._draftDate,
				createdRoutineId,
				scope,
			);
			editingRoutine.value = null;
			saved = true;
		} catch (error) {
			if (isStaleSeriesError(error) && editingEntry.value) {
				const entry = editingEntry.value;
				editingRoutine.value = null;
				toast({
					title: t('dailyRoutine.page.staleSeries'),
					variant: 'destructive',
				});
				try {
					await nextTick();
					editingRoutine.value = await loadRoutineForEdit(entry);
				} catch (reloadError) {
					editorError.value = errorText(reloadError);
				}
			} else {
				editorError.value = errorText(error);
			}
		} finally {
			editorBusy.value = false;
		}
		if (saved || createdRoutineId.value) await reload();
	}

	function onScopeSelected(scope: RoutineScope) {
		const draft = pendingDraft.value;
		pendingDraft.value = null;
		if (draft) onSaveRoutine(draft, scope);
	}

	async function onDuplicate(entry: RoutineEntry) {
		try {
			const draft = routineDraftFrom(
				await loadRoutineForEdit(entry),
				todayIso.value,
			);
			await saveRoutine({
				...draft,
				id: null,
				title: t('dailyRoutine.page.copyTitle', { title: draft.title }),
			});
		} catch (error) {
			toast({
				title: t('dailyRoutine.page.duplicateError'),
				description: errorText(error),
				variant: 'destructive',
			});
		}
		await reload();
	}

	async function onMoveRoutine(payload: {
		entry: RoutineEntry;
		date: string;
		timeH?: number;
		timeM?: number;
		allDay?: boolean;
		unscheduled?: boolean;
	}) {
		await store.dispatch('dailyRoutines/moveRoutine', payload);
		await reload();
	}

	// Drag the block's top/bottom edge in the day view: bottom changes duration,
	// top shifts the start time (and thus duration too).
	async function onResizeRoutine(payload: {
		entry: RoutineEntry;
		startMin: number;
		endMin: number;
	}) {
		const { entry, startMin, endMin } = payload;
		const durationMin = Math.max(15, endMin - startMin);
		if (
			!isLocalWorkspace.value &&
			entry.frequency &&
			entry.frequency !== 'NONE'
		) {
			await store.dispatch('dailyRoutines/resizeRecurring', {
				entry,
				startMin,
				durationMin,
			});
			await reload();
			return;
		}
		// Duration → task.approximately_time via a partial task update so
		// checkpoints/status/category are untouched. It's the single source of
		// truth the expander reads for the block height.
		await updateTask(entry.task_id, { approximately_time: durationMin } as any);
		// Top-edge drag also moved the start — reschedule that occurrence
		// (handles both one-off and recurring instances).
		const [origH, origM] = (entry.time ?? '00:00').split(':').map(Number);
		const origStartMin = (origH || 0) * 60 + (origM || 0);
		if (startMin !== origStartMin) {
			await store.dispatch('dailyRoutines/moveRoutine', {
				entry,
				date: entry.date,
				timeH: Math.floor(startMin / 60),
				timeM: startMin % 60,
			});
		}
		await reload();
	}

	function onDeleteRoutine(routine: any) {
		const entry = editingEntry.value;
		deleteTarget.value = {
			taskId: routine.id,
			title: routine.title,
			fromEditor: true,
			recurring:
				!isLocalWorkspace.value &&
				!!entry?.frequency &&
				entry.frequency !== 'NONE',
			occurrenceDate: routine.scheduled_date ?? todayIso.value,
			commentsCount: isLocalWorkspace.value ? 0 : (routine.comments_count ?? 0),
		};
		deleteDialogOpen.value = true;
	}

	async function onConvertRoutine(payload: {
		taskId: number;
		workspaceId: number;
		projectCategoryId: number | null;
	}) {
		const task = await store.dispatch('dailyRoutines/convertRoutine', payload);
		editingRoutine.value = null;
		convertingEntry.value = null;
		await reload();
		convertedTaskId.value = task.id;
		showConvertedTaskPrompt.value = true;
	}

	const convertingEntry = ref<RoutineEntry | null>(null);
	const convertBusy = ref(false);

	async function onConvertFromMenu(payload: {
		taskId: number;
		workspaceId: number;
		projectCategoryId: number | null;
	}) {
		convertBusy.value = true;
		try {
			await onConvertRoutine(payload);
		} catch (error) {
			toast({
				title: t('dailyRoutine.page.convertError'),
				description: errorText(error),
				variant: 'destructive',
			});
		} finally {
			convertBusy.value = false;
		}
	}

	const convertedTaskId = ref<number | null>(null);
	const showConvertedTaskPrompt = ref(false);

	function openConvertedTask() {
		showConvertedTaskPrompt.value = false;
		if (convertedTaskId.value) {
			store.commit('setCurrentTaskIdForModal', convertedTaskId.value);
		}
	}

	function triggerImport() {
		fileRef.value?.click();
	}
	async function onImportFile(e: Event) {
		const input = e.target as HTMLInputElement;
		const file = input.files?.[0];
		if (!file) return;
		const result = await store.dispatch('dailyRoutines/importIcs', {
			file,
			mode: 'skip',
		});
		input.value = '';
		const errs =
			Array.isArray(result.errors) && result.errors.length
				? t('dailyRoutine.page.importErrors', {
						list: result.errors.slice(0, 5).join('\n- '),
					})
				: '';
		// eslint-disable-next-line no-alert
		alert(
			t('dailyRoutine.page.importSummary', {
				created: result.created,
				skipped: result.skipped,
				replaced: result.replaced,
				errors: errs,
			}),
		);
		await reload();
	}
	async function onExport() {
		await store.dispatch('dailyRoutines/exportIcs');
	}
</script>
