<template>
	<Teleport to="body">
		<div
			class="fixed inset-0 z-[10000] flex items-center justify-center bg-black/55 p-5"
			@click.self="$emit('close')"
		>
			<div
				class="flex max-h-[calc(100vh-40px)] w-full max-w-[520px] flex-col overflow-hidden rounded-card border border-line bg-surface shadow-tmgr-lg"
				@keydown.esc="$emit('close')"
			>
				<!-- Header -->
				<div class="flex items-center gap-3 border-b border-line px-5 py-3.5">
					<span
						class="h-2 w-2 shrink-0 rounded-pill"
						:style="{ background: catColor, boxShadow: `0 0 12px ${catColor}55` }"
					/>
					<div class="min-w-0 flex-1">
						<div class="text-[10px] font-bold uppercase tracking-wider text-ink-subtle">
							{{ isNew ? 'New routine' : 'Edit routine' }}
						</div>
						<div class="truncate text-sm font-semibold text-ink">
							{{ draft.title || '(untitled)' }}
						</div>
					</div>
					<button
						type="button"
						class="flex h-7 w-7 items-center justify-center rounded-md bg-surface-sunken hover:bg-surface-hover"
						@click="$emit('close')"
					>
						<DRIcon name="close" :size="13" stroke="currentColor" />
					</button>
				</div>

				<!-- Body -->
				<div class="flex-1 overflow-auto p-4">
					<div class="flex flex-col gap-3.5">
						<!-- Title -->
						<ERSection label="Title">
							<input
								ref="titleRef"
								v-model="draft.title"
								type="text"
								placeholder="What's the routine?"
								class="w-full rounded-card border border-line bg-surface-sunken px-3.5 py-3 text-sm text-ink outline-none focus:border-line-strong"
							/>
						</ERSection>

						<!-- Description -->
						<ERSection label="Description" :hint="draft.description ? `${draft.description.length} chars` : 'Optional'">
							<textarea
								v-model="draft.description"
								rows="3"
								placeholder="Notes, context, links, checklist…"
								class="min-h-[64px] max-h-[220px] w-full resize-y rounded-card border border-line bg-surface-sunken px-3.5 py-2.5 text-sm leading-relaxed text-ink outline-none focus:border-line-strong"
							/>
						</ERSection>

						<!-- Category -->
						<ERSection label="Category" :hint="catName">
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
									<span class="h-1.5 w-1.5 rounded-pill" :style="{ background: c.color }" />
									{{ c.name }}
								</button>
							</div>
						</ERSection>

						<!-- Time + Duration -->
						<div class="grid grid-cols-2 gap-2.5">
							<ERSection label="Time">
								<div class="flex items-center gap-1.5 rounded-card border border-line bg-surface-sunken px-3 py-2.5">
									<DRIcon name="clock" :size="13" :stroke="catColor" />
									<input
										v-model.number="draft.timeH"
										type="number"
										min="0"
										max="23"
										class="w-9 bg-transparent text-center font-mono text-sm font-semibold text-ink outline-none"
									/>
									<span class="text-sm font-semibold text-ink-subtle">:</span>
									<input
										v-model.number="draft.timeM"
										type="number"
										min="0"
										max="59"
										step="5"
										class="w-9 bg-transparent text-center font-mono text-sm font-semibold text-ink outline-none"
									/>
								</div>
							</ERSection>
							<ERSection label="Duration">
								<div class="flex items-center gap-1.5 rounded-card border border-line bg-surface-sunken px-2.5 py-2">
									<DRIcon name="hourglass" :size="13" stroke="currentColor" />
									<input
										v-model.number="draft.durationMin"
										type="number"
										min="5"
										max="600"
										step="5"
										class="w-12 bg-transparent text-left font-mono text-sm font-semibold text-ink outline-none"
									/>
									<span class="text-xs text-ink-subtle">min</span>
									<div class="flex-1" />
									<div class="flex gap-0.5">
										<button
											v-for="p in [15, 30, 45, 60, 90]"
											:key="p"
											type="button"
											class="rounded-md px-1.5 py-1 text-[10px] font-semibold transition-colors"
											:class="
												draft.durationMin === p
													? 'bg-surface-hover text-ink'
													: 'text-ink-subtle hover:text-ink'
											"
											@click="draft.durationMin = p"
										>
											{{ p }}
										</button>
									</div>
								</div>
							</ERSection>
						</div>

						<!-- Recurrence -->
						<ERSection label="Repeats">
							<div class="flex gap-1 rounded-card border border-line bg-surface-sunken p-1">
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
									{{ f.label }}
								</button>
							</div>

							<div v-if="draft.frequency === 'WEEKLY'" class="mt-2.5 flex gap-1">
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

							<div v-if="draft.frequency === 'MONTHLY'" class="mt-2.5 flex items-center gap-2.5 text-xs text-ink-subtle">
								<span>On day</span>
								<div class="inline-flex items-center rounded-md border border-line bg-surface-sunken p-0.5">
									<button
										type="button"
										class="h-6 w-6 rounded-md text-base text-ink-subtle hover:text-ink"
										@click="draft.dayOfMonth = Math.max(1, draft.dayOfMonth - 1)"
									>
										−
									</button>
									<span class="min-w-[32px] text-center font-mono text-sm font-semibold text-ink">
										{{ draft.dayOfMonth }}
									</span>
									<button
										type="button"
										class="h-6 w-6 rounded-md text-base text-ink-subtle hover:text-ink"
										@click="draft.dayOfMonth = Math.min(31, draft.dayOfMonth + 1)"
									>
										+
									</button>
								</div>
								<span>of every month</span>
							</div>

							<div
								v-if="draft.frequency !== 'NONE'"
								class="mt-2.5 text-2xs italic text-ink-subtle"
							>
								{{ summary }}
							</div>
						</ERSection>

						<!-- Reminder -->
						<ERSection label="Reminder">
							<div class="flex flex-wrap gap-1.5">
								<button
									v-for="p in reminderPresets"
									:key="p.id"
									type="button"
									class="rounded-md border px-3 py-1.5 text-xs font-medium transition-colors"
									:class="
										draft.reminderMin === p.min
											? 'border-brand bg-brand-bg text-ink'
											: 'border-line bg-surface-sunken text-ink-subtle hover:text-ink'
									"
									@click="draft.reminderMin = p.min"
								>
									{{ p.label }}
								</button>
							</div>
						</ERSection>
					</div>
				</div>

				<!-- Footer -->
				<div class="flex items-center gap-2.5 border-t border-line p-3.5">
					<button
						v-if="!isNew"
						type="button"
						class="flex h-9 w-9 items-center justify-center rounded-card border border-status-fix-bg bg-status-fix-bg/40 text-status-fix-fg hover:bg-status-fix-bg/70"
						@click="onDelete"
					>
						<DRIcon name="trash" :size="14" stroke="currentColor" />
					</button>
					<button
						type="button"
						class="rounded-card border border-line bg-transparent px-4 py-2 text-sm text-ink-subtle hover:bg-surface-hover hover:text-ink"
						@click="$emit('close')"
					>
						Cancel
					</button>
					<button
						type="button"
						class="flex-1 rounded-card bg-brand px-4 py-2 text-sm font-semibold text-white shadow-tmgr-md transition-colors hover:bg-brand-hover disabled:cursor-not-allowed disabled:bg-surface-sunken disabled:text-ink-subtle disabled:shadow-none"
						:disabled="!draft.title.trim()"
						@click="onSave"
					>
						{{ isNew ? 'Create routine' : 'Save changes' }}
					</button>
				</div>
			</div>
		</div>
	</Teleport>
</template>

<script setup lang="ts">
	import { computed, onMounted, onBeforeUnmount, reactive, ref } from 'vue';
	import DRIcon from './DRIcon.vue';
	import ERSection from './ERSection.vue';
	import { ROUTINE_CATEGORY_LIST, resolveCategory } from '@/utils/dailyRoutines/categoryMap';
	import type { RoutineCategoryId, RoutineFrequency } from '@/types/dailyRoutine';

	interface RoutineDraft {
		id: number | null;
		title: string;
		description: string;
		cat: RoutineCategoryId;
		timeH: number;
		timeM: number;
		durationMin: number;
		frequency: RoutineFrequency;
		interval: number;
		daysOfWeek: number[];
		dayOfMonth: number;
		month: number;
		reminderMin: number | null;
	}

	const props = defineProps<{
		routine: any | null;
	}>();

	const emit = defineEmits<{
		(e: 'close'): void;
		(e: 'save', draft: RoutineDraft): void;
		(e: 'delete', routine: any): void;
	}>();

	const titleRef = ref<HTMLInputElement | null>(null);

	const initial: RoutineDraft = {
		id: props.routine?.id ?? null,
		title: props.routine?.title ?? '',
		description: props.routine?.description ?? '',
		cat: (props.routine?.routine_category?.id ?? props.routine?.cat ?? 'none') as RoutineCategoryId,
		timeH: props.routine?.time?.h ?? props.routine?.recurrence?.time?.hours ?? 9,
		timeM: props.routine?.time?.m ?? props.routine?.recurrence?.time?.minutes ?? 0,
		durationMin: props.routine?.recurrence?.duration_min ?? props.routine?.durationMin ?? 30,
		frequency: (props.routine?.recurrence?.frequency ?? props.routine?.frequency ?? 'NONE') as RoutineFrequency,
		interval: props.routine?.recurrence?.interval ?? 1,
		daysOfWeek: dowsToIndices(props.routine?.recurrence?.days_of_week ?? []),
		dayOfMonth: props.routine?.recurrence?.day_of_frequency ?? new Date().getDate(),
		month: props.routine?.recurrence?.month ?? 0,
		reminderMin: props.routine?.recurrence?.reminder_min ?? null,
	};

	const draft = reactive<RoutineDraft>(initial);
	const isNew = computed(() => !props.routine?.id);

	const categories = ROUTINE_CATEGORY_LIST;
	const catColor = computed(() => resolveCategory(draft.cat).color);
	const catName = computed(() => resolveCategory(draft.cat).name);
	const dowLabels = ['Mo', 'Tu', 'We', 'Th', 'Fr', 'Sa', 'Su'];

	const freqOptions: { id: RoutineFrequency; label: string }[] = [
		{ id: 'NONE', label: 'Once' },
		{ id: 'DAILY', label: 'Daily' },
		{ id: 'WEEKLY', label: 'Weekly' },
		{ id: 'MONTHLY', label: 'Monthly' },
		{ id: 'YEARLY', label: 'Yearly' },
	];

	const reminderPresets = [
		{ id: 'off', label: 'Off', min: null as number | null },
		{ id: 'at', label: 'At time', min: 0 as number | null },
		{ id: '5', label: '5m', min: 5 },
		{ id: '15', label: '15m', min: 15 },
		{ id: '30', label: '30m', min: 30 },
		{ id: '60', label: '1h', min: 60 },
	];

	function dowsToIndices(arr: any[]): number[] {
		const KEYS = ['MON', 'TUE', 'WED', 'THU', 'FRI', 'SAT', 'SUN'];
		if (!arr) return [];
		return arr
			.map((d: any) => (typeof d === 'number' ? d : KEYS.indexOf(String(d).toUpperCase())))
			.filter((i: number) => i >= 0)
			.sort();
	}

	function toggleDow(i: number) {
		if (draft.daysOfWeek.includes(i)) {
			draft.daysOfWeek = draft.daysOfWeek.filter(d => d !== i);
		} else {
			draft.daysOfWeek = [...draft.daysOfWeek, i].sort();
		}
	}

	const summary = computed(() => {
		const d = draft;
		if (d.frequency === 'DAILY') return 'Every day';
		if (d.frequency === 'WEEKLY') {
			if (d.daysOfWeek.length === 0) return 'Pick at least one day';
			if (d.daysOfWeek.length === 7) return 'Every day of the week';
			if (d.daysOfWeek.join(',') === '0,1,2,3,4') return 'Every weekday (Mon–Fri)';
			if (d.daysOfWeek.join(',') === '5,6') return 'Every weekend';
			return 'On ' + d.daysOfWeek.map(i => dowLabels[i]).join(', ');
		}
		if (d.frequency === 'MONTHLY') return `On day ${d.dayOfMonth} of every month`;
		if (d.frequency === 'YEARLY') return 'Annually';
		return '';
	});

	function onSave() {
		if (!draft.title.trim()) return;
		emit('save', { ...draft });
		emit('close');
	}

	function onDelete() {
		emit('delete', props.routine);
		emit('close');
	}

	function onKey(e: KeyboardEvent) {
		if (e.key === 'Escape') emit('close');
	}

	onMounted(() => {
		document.addEventListener('keydown', onKey);
		setTimeout(() => titleRef.value?.focus(), 50);
	});
	onBeforeUnmount(() => document.removeEventListener('keydown', onKey));
</script>
