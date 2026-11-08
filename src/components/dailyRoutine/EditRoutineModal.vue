<template>
	<Teleport to="body">
		<div
			class="fixed inset-0 z-[10000] flex items-center justify-center bg-black/55 p-3 pb-[max(0.75rem,env(safe-area-inset-bottom))] sm:p-5"
			@click.self="requestClose"
		>
			<div
				class="flex max-h-[calc(100vh-40px)] w-full max-w-[520px] flex-col overflow-hidden rounded-card border border-line bg-surface shadow-tmgr-lg supports-[height:100dvh]:max-h-[calc(100dvh-24px)]"
				@keydown.esc="requestClose"
			>
				<!-- Header -->
				<div class="flex items-center gap-3 border-b border-line px-5 py-3.5">
					<span
						class="h-2 w-2 shrink-0 rounded-pill"
						:style="{
							background: catColor,
							boxShadow: `0 0 12px ${catColor}55`,
						}"
					/>
					<div class="min-w-0 flex-1">
						<div
							class="text-[10px] font-bold tracking-wider text-ink-subtle uppercase"
						>
							{{
								isNew
									? $t('dailyRoutine.editModal.headerNew')
									: $t('dailyRoutine.editModal.headerEdit')
							}}
						</div>
						<div class="truncate text-sm font-semibold text-ink">
							{{ draft.title || $t('dailyRoutine.editModal.untitled') }}
						</div>
					</div>
					<button
						type="button"
						:aria-label="$t('common.actions.close')"
						class="flex h-7 w-7 items-center justify-center rounded-md bg-surface-sunken hover:bg-surface-hover"
						@click="requestClose"
					>
						<DRIcon name="close" :size="13" stroke="currentColor" />
					</button>
				</div>

				<!-- Body -->
				<div class="flex-1 overflow-auto p-4">
					<div class="flex flex-col gap-3.5">
						<!-- Title -->
						<ERSection :label="$t('dailyRoutine.editModal.fields.title')">
							<input
								ref="titleRef"
								v-model="draft.title"
								type="text"
								:aria-label="$t('dailyRoutine.editModal.fields.title')"
								:placeholder="
									$t('dailyRoutine.editModal.fields.titlePlaceholder')
								"
								class="w-full rounded-card border border-line bg-surface-sunken px-3.5 py-3 text-sm text-ink outline-hidden focus:border-line-strong"
							/>
						</ERSection>

						<!-- Description -->
						<ERSection
							:label="$t('dailyRoutine.editModal.fields.description')"
							:hint="
								draft.description
									? $t('dailyRoutine.editModal.fields.chars', {
											n: draft.description.length,
										})
									: $t('dailyRoutine.editModal.fields.optional')
							"
						>
							<textarea
								v-model="draft.description"
								rows="3"
								:aria-label="$t('dailyRoutine.editModal.fields.description')"
								:placeholder="
									$t('dailyRoutine.editModal.fields.descriptionPlaceholder')
								"
								class="max-h-[220px] min-h-[64px] w-full resize-y rounded-card border border-line bg-surface-sunken px-3.5 py-2.5 text-sm leading-relaxed text-ink outline-hidden focus:border-line-strong"
							/>
						</ERSection>

						<!-- Category -->
						<ERSection
							:label="$t('dailyRoutine.editModal.fields.category')"
							:hint="catName"
						>
							<div class="flex flex-wrap gap-1.5">
								<button
									v-for="c in categories"
									:key="c.id"
									type="button"
									class="inline-flex items-center gap-1.5 rounded-pill border px-3 py-1.5 text-xs font-medium transition-colors"
									:style="{
										borderColor: draft.cat === c.id ? c.color : undefined,
										background: draft.cat === c.id ? c.color + '22' : undefined,
									}"
									:class="
										draft.cat === c.id
											? 'text-ink'
											: 'border-line bg-surface-sunken text-ink-subtle hover:text-ink'
									"
									@click="draft.cat = c.id"
								>
									<span
										class="h-1.5 w-1.5 rounded-pill"
										:style="{ background: c.color }"
									/>
									{{ c.name }}
								</button>
							</div>
						</ERSection>

						<!-- Schedule mode (only for non-recurring) -->
						<ERSection
							v-if="draft.frequency === 'NONE'"
							:label="$t('dailyRoutine.editModal.fields.schedule')"
						>
							<label
								class="flex cursor-pointer items-center gap-2 rounded-card border border-line bg-surface-sunken px-3 py-2.5 text-sm text-ink"
							>
								<input
									v-model="draft.unscheduled"
									type="checkbox"
									class="h-4 w-4 accent-brand"
								/>
								<span class="flex-1">{{
									$t('dailyRoutine.editModal.fields.unscheduled')
								}}</span>
							</label>
						</ERSection>

						<!-- Date (non-recurring + scheduled) -->
						<ERSection
							v-if="draft.frequency === 'NONE' && !draft.unscheduled"
							:label="$t('dailyRoutine.editModal.fields.date')"
						>
							<input
								v-model="draft.scheduledDate"
								type="date"
								class="w-full rounded-card border border-line bg-surface-sunken px-3.5 py-2.5 text-sm text-ink outline-hidden focus:border-line-strong"
							/>
						</ERSection>

						<!-- Time range: start → end, with the span shown -->
						<ERSection
							v-if="!(draft.frequency === 'NONE' && draft.unscheduled)"
							:label="$t('dailyRoutine.editModal.fields.time')"
						>
							<div class="flex flex-col gap-2.5">
								<div class="flex items-end gap-2">
									<label class="flex flex-1 flex-col gap-1">
										<span
											class="text-[10px] font-bold tracking-wider text-ink-subtle uppercase"
											>{{ $t('dailyRoutine.editModal.fields.start') }}</span
										>
										<div
											class="flex items-center gap-1.5 rounded-card border border-line bg-surface-sunken px-3 py-2.5"
										>
											<input
												v-model="startStr"
												type="time"
												step="900"
												class="dr-time w-full bg-transparent font-mono text-sm font-semibold text-ink outline-hidden"
											/>
											<DRIcon name="clock" :size="13" :stroke="catColor" />
										</div>
									</label>
									<span class="pb-2.5 text-ink-subtle">→</span>
									<label class="flex flex-1 flex-col gap-1">
										<span
											class="text-[10px] font-bold tracking-wider text-ink-subtle uppercase"
											>{{ $t('dailyRoutine.editModal.fields.end') }}</span
										>
										<div
											class="flex items-center gap-1.5 rounded-card border border-line bg-surface-sunken px-3 py-2.5"
										>
											<input
												v-model="endStr"
												type="time"
												step="900"
												class="dr-time w-full bg-transparent font-mono text-sm font-semibold text-ink outline-hidden"
											/>
											<DRIcon name="clock" :size="13" :stroke="catColor" />
										</div>
									</label>
								</div>
								<div>
									<span
										class="inline-flex items-center gap-1.5 rounded-pill border px-2.5 py-1 text-xs font-semibold"
										:style="{
											color: catColor,
											borderColor: catColor + '73',
											background: catColor + '24',
										}"
									>
										<DRIcon name="hourglass" :size="12" :stroke="catColor" />
										{{ durationLabel }}
									</span>
								</div>
							</div>
						</ERSection>

						<!-- Recurrence -->
						<ERSection :label="$t('dailyRoutine.editModal.fields.repeats')">
							<div
								class="flex gap-1 rounded-card border border-line bg-surface-sunken p-1"
							>
								<button
									v-for="f in freqOptions"
									:key="f.id"
									type="button"
									class="flex-1 rounded-md py-1.5 text-xs font-medium transition-colors"
									:class="
										draft.frequency === f.id
											? 'bg-surface text-ink shadow-tmgr-xs'
											: 'text-ink-subtle hover:text-ink'
									"
									@click="draft.frequency = f.id"
								>
									{{ $t(f.labelKey) }}
								</button>
							</div>

							<div
								v-if="draft.frequency === 'WEEKLY'"
								class="mt-2.5 flex gap-1"
							>
								<button
									v-for="(lbl, i) in dowLabels"
									:key="i"
									type="button"
									class="flex-1 rounded-md border py-2 text-2xs font-semibold transition-colors"
									:style="
										draft.daysOfWeek.includes(i)
											? { borderColor: catColor, background: catColor + '33' }
											: undefined
									"
									:class="
										draft.daysOfWeek.includes(i)
											? 'text-ink'
											: 'border-line bg-surface-sunken text-ink-subtle hover:text-ink'
									"
									@click="toggleDow(i)"
								>
									{{ lbl }}
								</button>
							</div>

							<div
								v-if="draft.frequency === 'MONTHLY'"
								class="mt-2.5 flex items-center gap-2.5 text-xs text-ink-subtle"
							>
								<span>{{ $t('dailyRoutine.editModal.fields.onDay') }}</span>
								<div
									class="inline-flex items-center rounded-md border border-line bg-surface-sunken p-0.5"
								>
									<button
										type="button"
										class="h-6 w-6 rounded-md text-base text-ink-subtle hover:text-ink"
										@click="
											draft.dayOfMonth = Math.max(1, draft.dayOfMonth - 1)
										"
									>
										−
									</button>
									<span
										class="min-w-[32px] text-center font-mono text-sm font-semibold text-ink"
									>
										{{ draft.dayOfMonth }}
									</span>
									<button
										type="button"
										class="h-6 w-6 rounded-md text-base text-ink-subtle hover:text-ink"
										@click="
											draft.dayOfMonth = Math.min(31, draft.dayOfMonth + 1)
										"
									>
										+
									</button>
								</div>
								<span>{{
									$t('dailyRoutine.editModal.fields.ofEveryMonth')
								}}</span>
							</div>

							<div
								v-if="draft.frequency !== 'NONE'"
								class="mt-2.5 text-2xs text-ink-subtle italic"
							>
								{{ summary }}
							</div>
						</ERSection>

						<!-- Reminder -->
						<ERSection
							:label="$t('dailyRoutine.editModal.fields.reminder')"
							data-tour="routine.reminder"
							:hint="
								draft.frequency === 'NONE'
									? $t('dailyRoutine.editModal.fields.reminderOnlyRepeating')
									: undefined
							"
						>
							<div class="flex flex-wrap gap-1.5">
								<button
									v-for="p in reminderPresets"
									:key="p.id"
									type="button"
									:disabled="draft.frequency === 'NONE'"
									class="rounded-md border px-3 py-1.5 text-xs font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50"
									:class="
										draft.frequency !== 'NONE' && draft.reminderMin === p.min
											? 'border-brand bg-brand-bg text-ink'
											: 'border-line bg-surface-sunken text-ink-subtle hover:text-ink'
									"
									@click="draft.reminderMin = p.min"
								>
									{{ $t(p.labelKey) }}
								</button>
							</div>
						</ERSection>

						<!-- Convert to task -->
						<ERSection
							v-if="!isNew"
							:label="$t('dailyRoutine.convert.title')"
							data-tour="routine.to-task"
						>
							<button
								v-if="!convertOpen"
								type="button"
								class="rounded-md border border-line bg-surface-sunken px-3 py-1.5 text-xs font-medium text-ink-subtle transition-colors hover:text-ink"
								@click="openConvert"
							>
								{{ $t('dailyRoutine.editModal.fields.moveToWorkspace') }}
							</button>
							<ConvertRoutineFields v-else @convert="onConvert" />
						</ERSection>
					</div>
				</div>

				<!-- Timestamps -->
				<div
					v-if="!isNew && (createdAtLabel || updatedAtLabel)"
					class="flex items-center gap-3 px-3.5 pb-2 text-2xs text-ink-subtle"
				>
					<span v-if="createdAtLabel">{{
						$t('dailyRoutine.editModal.created', { when: createdAtLabel })
					}}</span>
					<span v-if="updatedAtLabel && !updatedSameAsCreated">{{
						$t('dailyRoutine.editModal.updated', { when: updatedAtLabel })
					}}</span>
				</div>

				<div
					v-if="error"
					role="alert"
					class="mx-3.5 mb-2 rounded-card border border-status-fix-fg/40 bg-status-fix-bg px-3 py-2 text-xs text-status-fix-fg"
				>
					{{ error }}
				</div>

				<!-- Footer -->
				<div class="flex items-center gap-2.5 border-t border-line p-3.5">
					<button
						v-if="!isNew"
						type="button"
						:aria-label="$t('dailyRoutine.editModal.deleteRoutine')"
						:disabled="busy"
						class="flex h-9 w-9 items-center justify-center rounded-card border border-status-fix-bg bg-status-fix-bg/40 text-status-fix-fg hover:bg-status-fix-bg/70 disabled:cursor-not-allowed disabled:opacity-50"
						@click="onDelete"
					>
						<DRIcon name="trash" :size="14" stroke="currentColor" />
					</button>
					<button
						type="button"
						class="rounded-card border border-line bg-transparent px-4 py-2 text-sm text-ink-subtle hover:bg-surface-hover hover:text-ink"
						@click="requestClose"
					>
						{{ $t('common.actions.cancel') }}
					</button>
					<button
						type="button"
						class="flex-1 rounded-card bg-brand px-4 py-2 text-sm font-semibold text-on-accent shadow-tmgr-md transition-colors hover:bg-brand-hover disabled:cursor-not-allowed disabled:bg-surface-sunken disabled:text-ink-subtle disabled:shadow-none"
						:disabled="!draft.title.trim() || busy"
						@click="onSave"
					>
						{{
							isNew
								? $t('dailyRoutine.editModal.createRoutine')
								: $t('dailyRoutine.editModal.saveChanges')
						}}
					</button>
				</div>
			</div>
		</div>
	</Teleport>
</template>

<script setup lang="ts">
	import { currentLocale, t } from '@/i18n';
	import { emitRoutineEditorOpened } from '@/tours/pageRoutineEvents';
	import type { RoutineFrequency } from '@/types/dailyRoutine';
	import {
		ROUTINE_CATEGORY_LIST,
		resolveCategory,
	} from '@/utils/dailyRoutines/categoryMap';
	import { isSameTimestamp } from '@/utils/dailyRoutines/relativeTime';
	import { currentTimeZone, todayIn } from '@/utils/timezone';
	import {
		computed,
		onBeforeUnmount,
		onMounted,
		reactive,
		ref,
		watch,
	} from 'vue';
	import ConvertRoutineFields from './ConvertRoutineFields.vue';
	import DRIcon from './DRIcon.vue';
	import ERSection from './ERSection.vue';
	import { routineDraftFrom, type RoutineDraft } from './routinePayload';

	const props = defineProps<{
		routine: any | null;
		busy?: boolean;
		error?: string;
		confirmOpen?: boolean;
	}>();

	const emit = defineEmits<{
		(e: 'close'): void;
		(e: 'save', draft: RoutineDraft): void;
		(e: 'delete', routine: any): void;
		(
			e: 'convert',
			payload: {
				taskId: number;
				workspaceId: number;
				projectCategoryId: number | null;
			},
		): void;
	}>();

	const titleRef = ref<HTMLInputElement | null>(null);

	const initial = routineDraftFrom(
		props.routine ?? {},
		todayIn(currentTimeZone()),
	);

	const draft = reactive<RoutineDraft>(initial);
	const isNew = computed(() => !props.routine?.id);

	// ── start / end time range ─────────────────────────────────────────────────
	// The form stores a start (timeH/timeM) + durationMin; the UI edits start and
	// end, deriving the span. Editing start keeps the end fixed (duration shrinks/
	// grows); editing end keeps the start. Minimum span is 15 minutes.
	const MIN_DURATION = 15;
	const pad2 = (n: number) => String(n).padStart(2, '0');
	const startMin = () => draft.timeH * 60 + draft.timeM;
	// End is stored independently so editing one field never rewrites the other —
	// a native time input fires on every intermediate keystroke (typing "16:00"
	// passes through "01:00"), and cross-field adjustment there corrupted the start.
	const endMinRef = ref(startMin() + Math.max(MIN_DURATION, draft.durationMin));

	const startStr = computed<string>({
		get: () => `${pad2(draft.timeH)}:${pad2(draft.timeM)}`,
		set: (v) => {
			const [h, m] = String(v).split(':').map(Number);
			if (Number.isNaN(h) || Number.isNaN(m)) return;
			draft.timeH = h;
			draft.timeM = m;
		},
	});

	const endStr = computed<string>({
		get: () =>
			`${pad2(Math.floor(endMinRef.value / 60) % 24)}:${pad2(
				endMinRef.value % 60,
			)}`,
		set: (v) => {
			const [h, m] = String(v).split(':').map(Number);
			if (Number.isNaN(h) || Number.isNaN(m)) return;
			endMinRef.value = h * 60 + m;
		},
	});

	// Keep the persisted duration in sync with the visible range (min 15 min).
	watch(
		[() => startMin(), endMinRef],
		() => {
			draft.durationMin = Math.max(MIN_DURATION, endMinRef.value - startMin());
		},
		{ immediate: true },
	);

	const durationLabel = computed(() => {
		const total = Math.max(MIN_DURATION, endMinRef.value - startMin());
		const h = Math.floor(total / 60);
		const m = total % 60;
		if (h && m) return t('dailyRoutine.editModal.duration.hm', { h, m });
		if (h) return t('dailyRoutine.editModal.duration.h', { h });
		return t('dailyRoutine.editModal.duration.m', { m });
	});

	const categories = ROUTINE_CATEGORY_LIST;
	const catColor = computed(() => resolveCategory(draft.cat).color);
	const catName = computed(() => resolveCategory(draft.cat).name);
	const DOW_LABEL_KEYS = [
		'dailyRoutine.dow.mo',
		'dailyRoutine.dow.tu',
		'dailyRoutine.dow.we',
		'dailyRoutine.dow.th',
		'dailyRoutine.dow.fr',
		'dailyRoutine.dow.sa',
		'dailyRoutine.dow.su',
	];
	const dowLabels = computed(() => DOW_LABEL_KEYS.map((key) => t(key)));

	const freqOptions: { id: RoutineFrequency; labelKey: string }[] = [
		{ id: 'NONE', labelKey: 'dailyRoutine.editModal.freq.once' },
		{ id: 'DAILY', labelKey: 'dailyRoutine.editModal.freq.daily' },
		{ id: 'WEEKLY', labelKey: 'dailyRoutine.editModal.freq.weekly' },
		{ id: 'MONTHLY', labelKey: 'dailyRoutine.editModal.freq.monthly' },
		{ id: 'YEARLY', labelKey: 'dailyRoutine.editModal.freq.yearly' },
	];

	const reminderPresets = [
		{
			id: 'off',
			labelKey: 'dailyRoutine.editModal.reminder.off',
			min: null as number | null,
		},
		{
			id: 'at',
			labelKey: 'dailyRoutine.editModal.reminder.atTime',
			min: 0 as number | null,
		},
		{ id: '5', labelKey: 'dailyRoutine.editModal.reminder.min5', min: 5 },
		{ id: '15', labelKey: 'dailyRoutine.editModal.reminder.min15', min: 15 },
		{ id: '30', labelKey: 'dailyRoutine.editModal.reminder.min30', min: 30 },
		{ id: '60', labelKey: 'dailyRoutine.editModal.reminder.hour1', min: 60 },
	];

	function toggleDow(i: number) {
		if (draft.daysOfWeek.includes(i)) {
			draft.daysOfWeek = draft.daysOfWeek.filter((d) => d !== i);
		} else {
			draft.daysOfWeek = [...draft.daysOfWeek, i].sort();
		}
	}

	const summary = computed(() => {
		const d = draft;
		if (d.frequency === 'DAILY')
			return t('dailyRoutine.editModal.summary.daily');
		if (d.frequency === 'WEEKLY') {
			if (d.daysOfWeek.length === 0)
				return t('dailyRoutine.editModal.summary.pickDay');
			if (d.daysOfWeek.length === 7)
				return t('dailyRoutine.editModal.summary.everyDayOfWeek');
			if (d.daysOfWeek.join(',') === '0,1,2,3,4')
				return t('dailyRoutine.editModal.summary.weekdays');
			if (d.daysOfWeek.join(',') === '5,6')
				return t('dailyRoutine.editModal.summary.weekends');
			return t('dailyRoutine.editModal.summary.onDays', {
				days: d.daysOfWeek.map((i) => dowLabels.value[i]).join(', '),
			});
		}
		if (d.frequency === 'MONTHLY')
			return t('dailyRoutine.editModal.summary.monthly', { day: d.dayOfMonth });
		if (d.frequency === 'YEARLY')
			return t('dailyRoutine.editModal.summary.yearly');
		return '';
	});

	function onSave() {
		if (!draft.title.trim() || props.busy) return;
		emit('save', { ...draft });
	}

	function onDelete() {
		emit('delete', props.routine);
	}

	function tsLabel(value: string | null | undefined): string {
		if (!value) return '';
		const d = new Date(value);
		if (isNaN(d.getTime())) return '';
		return d.toLocaleString(currentLocale(), {
			month: 'short',
			day: 'numeric',
			year: 'numeric',
			hour: '2-digit',
			minute: '2-digit',
		});
	}
	const createdAtLabel = computed(() => tsLabel(props.routine?.created_at));
	const updatedSameAsCreated = computed(() =>
		isSameTimestamp(props.routine?.created_at, props.routine?.updated_at),
	);
	const updatedAtLabel = computed(() => tsLabel(props.routine?.updated_at));

	// ── convert to task ────────────────────────────────────────────────────
	const convertOpen = ref(false);

	function openConvert() {
		convertOpen.value = true;
	}

	function onConvert(target: {
		workspaceId: number;
		projectCategoryId: number | null;
	}) {
		emit('convert', {
			taskId: props.routine.task_id ?? props.routine.id,
			...target,
		});
		emit('close');
	}

	function onKey(e: KeyboardEvent) {
		if (e.key === 'Escape' && !props.confirmOpen) requestClose();
	}

	function requestClose() {
		if (props.busy) return;
		emit('close');
	}

	onMounted(() => {
		document.addEventListener('keydown', onKey);
		emitRoutineEditorOpened(isNew.value);
		setTimeout(() => titleRef.value?.focus(), 50);
	});
	onBeforeUnmount(() => document.removeEventListener('keydown', onKey));
</script>

<style scoped>
	/* The DRIcon clock stands in for the native picker button, so hide the
	   browser's own indicator and spinners to avoid a doubled icon. */
	.dr-time::-webkit-calendar-picker-indicator {
		display: none;
	}
	.dr-time::-webkit-inner-spin-button,
	.dr-time::-webkit-clear-button {
		display: none;
	}
</style>
