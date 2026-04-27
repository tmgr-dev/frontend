<template>
	<div>
		<!-- Disabled state: Enable button -->
		<button
			v-if="!loading && !state"
			type="button"
			class="flex w-full items-center justify-center gap-2 rounded-card border border-dashed border-line bg-surface-sunken/40 px-3 py-3 text-xs font-medium text-ink-subtle transition hover:border-line-strong hover:text-ink"
			:disabled="enabling"
			@click="handleEnable"
		>
			<span aria-hidden="true">🍅</span>
			{{ enabling ? 'Enabling…' : 'Enable Pomodoro' }}
		</button>

		<!-- Active card -->
		<div
			v-else-if="state"
			class="relative overflow-hidden rounded-card border border-line bg-surface-sunken px-4 py-3.5"
		>
			<div
				class="pointer-events-none absolute inset-0 transition"
				:style="{
					background: state.running
						? `radial-gradient(160px 80px at 60px 50%, ${phaseMeta.soft}, transparent 70%)`
						: 'transparent',
				}"
			/>

			<div class="relative z-10 flex items-center gap-4">
				<div class="relative" :style="{ width: '88px', height: '88px' }">
					<svg
						width="88"
						height="88"
						class="-rotate-90"
					>
						<circle
							cx="44"
							cy="44"
							r="41.5"
							fill="none"
							class="stroke-line"
							stroke-width="5"
						/>
						<circle
							cx="44"
							cy="44"
							r="41.5"
							fill="none"
							:stroke="phaseMeta.color"
							stroke-width="5"
							stroke-linecap="round"
							:stroke-dasharray="ringCircumference"
							:stroke-dashoffset="ringOffset"
							class="transition-[stroke-dashoffset,stroke] duration-[1000ms] ease-linear"
						/>
					</svg>
					<div class="absolute inset-0 flex items-center justify-center">
						<button
							type="button"
							class="flex h-[60px] w-[60px] items-center justify-center rounded-full border-0 transition"
							:style="{
								background: state.running
									? 'var(--bg-hover)'
									: phaseMeta.soft,
							}"
							@click="toggle"
						>
							<Pause
								v-if="state.running"
								:size="20"
								:stroke-width="2.4"
								:color="phaseMeta.color"
							/>
							<span v-else class="text-2xl leading-none" aria-hidden="true">🍅</span>
						</button>
					</div>
				</div>

				<div class="relative z-10 min-w-0 flex-1">
					<div class="mb-1.5 flex items-center gap-2">
						<span
							class="inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 text-[10px] font-bold tracking-[.08em]"
							:style="{ background: phaseMeta.soft, color: phaseMeta.color }"
						>
							<span
								class="h-1.5 w-1.5 rounded-full"
								:style="{
									background: phaseMeta.color,
									boxShadow: `0 0 6px ${phaseMeta.color}`,
								}"
							/>
							{{ phaseMeta.label }}
						</span>
						<button
							type="button"
							class="flex h-[22px] w-[22px] items-center justify-center rounded-md hover:bg-surface-hover"
							title="Skip phase"
							@click="skip"
						>
							<SkipForward :size="11" class="text-ink-subtle" />
						</button>
						<button
							type="button"
							class="flex h-[22px] w-[22px] items-center justify-center rounded-md hover:bg-surface-hover"
							title="Reset"
							@click="reset"
						>
							<RotateCcw :size="11" class="text-ink-subtle" />
						</button>
						<div class="flex-1" />
						<button
							type="button"
							class="inline-flex items-center gap-1.5 rounded-md border-0 bg-[rgba(245,181,74,.1)] px-2 py-1 text-[10px] font-semibold tracking-[.05em] text-[#f5b54a] hover:bg-[rgba(245,181,74,.18)]"
							title="I got distracted"
							@click="handleDistract"
						>
							<Zap :size="11" />
							DISTRACTED{{
								state.distractions_count > 0 ? ` · ${state.distractions_count}` : ''
							}}
						</button>
						<button
							type="button"
							class="flex h-[26px] w-[26px] items-center justify-center rounded-md hover:bg-surface-hover"
							title="Pomodoro settings"
							@click="settingsOpen = true"
						>
							<Settings :size="13" class="text-ink-subtle" />
						</button>
						<button
							type="button"
							class="flex h-[26px] w-[26px] items-center justify-center rounded-md hover:bg-surface-hover"
							title="Disable Pomodoro for this task"
							@click="handleDisable"
						>
							<X :size="13" class="text-ink-subtle" />
						</button>
					</div>
					<div
						class="mb-2 tracking-wide"
						:style="{
							font: '600 28px/1 \'JetBrains Mono\', \'SF Mono\', ui-monospace, monospace',
							color: phaseMeta.color,
						}"
					>
						{{ formattedRemaining }}
					</div>
					<div class="flex items-center gap-2.5">
						<div class="flex items-center gap-1.5">
							<span
								v-for="(s, i) in dots"
								:key="i"
								class="inline-block h-[11px] w-[11px] rounded-full border"
								:style="dotStyle(s)"
							/>
						</div>
						<span
							class="text-[10px] font-bold tracking-[.08em] text-ink-muted"
						>
							{{ state.completed_pomodoros }} POMODOROS · {{ settings.focusMin }}/{{
								settings.shortMin
							}}/{{ settings.longMin }}
						</span>
					</div>
				</div>
			</div>
		</div>

		<PomodoroSettingsModal
			:open="settingsOpen"
			:value="settings"
			@close="settingsOpen = false"
			@save="onSettingsSaved"
		/>
	</div>
</template>

<script lang="ts">
	import { defineComponent, ref, computed, onBeforeUnmount, watch } from 'vue';
	import { Pause, SkipForward, RotateCcw, Zap, Settings, X } from 'lucide-vue-next';
	import {
		PomodoroPhase,
		PomodoroState,
		PomodoroSettings,
		DEFAULT_POMODORO_SETTINGS,
		getPomodoroSettings,
		getPomodoroState,
		enablePomodoro,
		disablePomodoro,
		updatePomodoroState,
		distractPomodoro,
	} from '@/actions/tmgr/pomodoro';
	import PomodoroSettingsModal from '@/components/tasks/PomodoroSettingsModal.vue';

	const PHASE_META: Record<
		PomodoroPhase,
		{ label: string; color: string; soft: string }
	> = {
		focus: { label: 'FOCUS', color: '#e85a4f', soft: 'rgba(232,90,79,.16)' },
		short: { label: 'SHORT BREAK', color: '#5b8cff', soft: 'rgba(91,140,255,.16)' },
		long: { label: 'LONG BREAK', color: '#a78bfa', soft: 'rgba(167,139,250,.18)' },
	};

	const phaseDurationMs = (phase: PomodoroPhase, s: PomodoroSettings): number => {
		if (phase === 'focus') return s.focusMin * 60_000;
		if (phase === 'short') return s.shortMin * 60_000;
		return s.longMin * 60_000;
	};

	const fmt = (ms: number): string => {
		const total = Math.max(0, Math.floor(ms / 1000));
		const m = Math.floor(total / 60);
		const s = total % 60;
		return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
	};

	export default defineComponent({
		name: 'PomodoroBlock',
		components: {
			PomodoroSettingsModal,
			Pause,
			SkipForward,
			RotateCcw,
			Zap,
			Settings,
			X,
		},
		props: {
			taskId: { type: Number, required: true },
		},
		setup(props) {
			const state = ref<PomodoroState | null>(null);
			const settings = ref<PomodoroSettings>({ ...DEFAULT_POMODORO_SETTINGS });
			const settingsOpen = ref(false);
			const loading = ref(true);
			const enabling = ref(false);

			const tickNow = ref(Date.now());
			let intervalId: ReturnType<typeof setInterval> | null = null;

			const startTicker = () => {
				if (intervalId) return;
				intervalId = setInterval(() => {
					tickNow.value = Date.now();
				}, 1000);
			};
			const stopTicker = () => {
				if (intervalId) {
					clearInterval(intervalId);
					intervalId = null;
				}
			};

			const computedRemainingMs = computed<number>(() => {
				const s = state.value;
				if (!s) return 0;
				if (!s.running || !s.phase_started_at) return s.remaining_ms;
				const startedAt = new Date(s.phase_started_at).getTime();
				const elapsed = tickNow.value - startedAt;
				return Math.max(0, s.remaining_ms - elapsed);
			});

			const phaseMeta = computed(() => {
				const p: PomodoroPhase = state.value?.phase ?? 'focus';
				return PHASE_META[p];
			});

			const phaseTotal = computed<number>(() => {
				return phaseDurationMs(state.value?.phase ?? 'focus', settings.value);
			});

			const ringCircumference = 2 * Math.PI * 41.5;
			const ringOffset = computed<number>(() => {
				const total = phaseTotal.value;
				if (!total) return ringCircumference;
				const progress = 1 - computedRemainingMs.value / total;
				const clamped = Math.max(0, Math.min(1, progress));
				return ringCircumference * (1 - clamped);
			});

			const formattedRemaining = computed(() => fmt(computedRemainingMs.value));

			const dots = computed<Array<'done' | 'active' | 'pending'>>(() => {
				if (!state.value) return [];
				const target = settings.value.longEvery;
				const completed = state.value.completed_pomodoros % target;
				const isActiveFocus = state.value.phase === 'focus';
				return Array.from({ length: target }, (_, i) => {
					if (i < completed) return 'done';
					if (i === completed && isActiveFocus) return 'active';
					return 'pending';
				});
			});

			const dotStyle = (s: 'done' | 'active' | 'pending') => {
				const accent = '#e85a4f';
				const bg = s === 'done' ? accent : 'transparent';
				const border = s === 'pending' ? 'var(--line-strong-color)' : accent;
				return {
					background: bg,
					borderColor: border,
					boxShadow: s === 'active' ? '0 0 10px rgba(232,90,79,.6)' : 'none',
				};
			};

			const refreshSettings = () => {
				settings.value = getPomodoroSettings();
			};

			const load = async () => {
				loading.value = true;
				try {
					refreshSettings();
					state.value = await getPomodoroState(props.taskId);
					if (state.value?.running) startTicker();
				} finally {
					loading.value = false;
				}
			};

			const persist = async (
				patch: Partial<
					Pick<
						PomodoroState,
						| 'phase'
						| 'running'
						| 'phase_started_at'
						| 'remaining_ms'
						| 'completed_pomodoros'
						| 'distractions_count'
					>
				>,
			) => {
				if (!state.value) return;
				const updated = await updatePomodoroState(props.taskId, patch);
				state.value = updated;
			};

			const start = async () => {
				if (!state.value) return;
				const remaining =
					state.value.remaining_ms > 0
						? state.value.remaining_ms
						: phaseTotal.value;
				await persist({
					running: true,
					phase_started_at: new Date().toISOString(),
					remaining_ms: remaining,
				});
				startTicker();
			};

			const pause = async () => {
				if (!state.value) return;
				const remaining = computedRemainingMs.value;
				stopTicker();
				await persist({
					running: false,
					phase_started_at: null,
					remaining_ms: remaining,
				});
			};

			const toggle = async () => {
				if (!state.value) return;
				if (state.value.running) await pause();
				else await start();
			};

			const advancePhase = async (autoStart: boolean) => {
				if (!state.value) return;
				const wasFocus = state.value.phase === 'focus';
				let nextPhase: PomodoroPhase;
				let nextCompleted = state.value.completed_pomodoros;
				if (wasFocus) {
					nextCompleted = state.value.completed_pomodoros + 1;
					nextPhase =
						nextCompleted % settings.value.longEvery === 0 ? 'long' : 'short';
				} else {
					nextPhase = 'focus';
				}
				const nextRemaining = phaseDurationMs(nextPhase, settings.value);
				stopTicker();
				await persist({
					phase: nextPhase,
					running: autoStart,
					phase_started_at: autoStart ? new Date().toISOString() : null,
					remaining_ms: nextRemaining,
					completed_pomodoros: nextCompleted,
				});
				if (autoStart) startTicker();
			};

			const skip = async () => {
				await advancePhase(false);
			};

			const reset = async () => {
				if (!state.value) return;
				stopTicker();
				const remaining = phaseDurationMs(state.value.phase, settings.value);
				await persist({
					running: false,
					phase_started_at: null,
					remaining_ms: remaining,
				});
			};

			const handleDistract = async () => {
				if (!state.value) return;
				const updated = await distractPomodoro(props.taskId);
				state.value = updated;
			};

			const handleEnable = async () => {
				enabling.value = true;
				try {
					refreshSettings();
					state.value = await enablePomodoro(props.taskId);
				} finally {
					enabling.value = false;
				}
			};

			const handleDisable = async () => {
				stopTicker();
				await disablePomodoro(props.taskId);
				state.value = null;
			};

			const onSettingsSaved = (next: PomodoroSettings) => {
				settings.value = next;
				if (state.value && !state.value.running) {
					const newRemaining = phaseDurationMs(state.value.phase, next);
					persist({ remaining_ms: newRemaining });
				}
			};

			watch(computedRemainingMs, (val: number) => {
				if (val <= 0 && state.value?.running) {
					advancePhase(settings.value.autoStart);
				}
			});

			watch(
				() => props.taskId,
				() => {
					stopTicker();
					load();
				},
				{ immediate: true },
			);

			onBeforeUnmount(() => {
				stopTicker();
			});

			return {
				state,
				settings,
				settingsOpen,
				loading,
				enabling,
				phaseMeta,
				ringCircumference,
				ringOffset,
				formattedRemaining,
				dots,
				dotStyle,
				toggle,
				skip,
				reset,
				handleDistract,
				handleEnable,
				handleDisable,
				onSettingsSaved,
			};
		},
	});
</script>
