<template>
	<div
		v-if="open"
		class="fixed inset-0 z-[120] flex items-center justify-center bg-black/60 px-4 py-8"
		@click.self="$emit('close')"
	>
		<div
			ref="dialogEl"
			class="w-[440px] max-w-full rounded-xl border border-line bg-surface p-6 text-ink shadow-2xl"
			role="dialog"
			aria-modal="true"
		>
			<div class="mb-5 flex items-start gap-3">
				<div class="flex-1">
					<div class="mb-1.5 flex items-center gap-2.5">
						<span class="text-xl">🍅</span>
						<h2 class="text-xl font-bold tracking-tight">Pomodoro</h2>
					</div>
					<p class="text-[13px] leading-snug text-ink-muted">
						Configure focus and break intervals. Settings apply to all your
						tasks.
					</p>
				</div>
				<button
					type="button"
					class="flex h-7 w-7 items-center justify-center rounded-md hover:bg-surface-hover"
					@click="$emit('close')"
					aria-label="Close"
				>
					<X :size="16" class="text-ink-subtle" />
				</button>
			</div>

			<div class="mb-3.5 grid grid-cols-2 gap-2.5">
				<SettingLabel>Focus duration</SettingLabel>
				<SettingLabel>Short break</SettingLabel>
				<TimeField
					:model-value="draft.focusMin"
					accent="#e85a4f"
					@update:model-value="set('focusMin', $event)"
				/>
				<TimeField
					:model-value="draft.shortMin"
					accent="#5b8cff"
					@update:model-value="set('shortMin', $event)"
				/>
				<SettingLabel>Long break</SettingLabel>
				<SettingLabel>Long break every</SettingLabel>
				<TimeField
					:model-value="draft.longMin"
					accent="#a78bfa"
					@update:model-value="set('longMin', $event)"
				/>
				<Stepper
					:model-value="draft.longEvery"
					:min="2"
					:max="12"
					:suffix="draft.longEvery === 1 ? 'pomodoro' : 'pomodoros'"
					@update:model-value="set('longEvery', $event)"
				/>
			</div>

			<div
				class="mb-3.5 overflow-hidden rounded-[10px] border border-line bg-surface-sunken"
			>
				<RowToggle
					title="Auto-start next interval"
					subtitle="Skip the play button between focus and break"
					:model-value="draft.autoStart"
					@update:model-value="set('autoStart', $event)"
				/>
				<div class="h-px bg-line" />
				<RowToggle
					title="Pause Time Tracked during break"
					subtitle="Stop the wall-clock while you rest"
					:model-value="draft.pauseTrackingOnBreak"
					@update:model-value="set('pauseTrackingOnBreak', $event)"
				/>
			</div>

			<div class="mb-3.5">
				<SettingLabel>Notifications</SettingLabel>
				<div
					class="grid grid-cols-3 gap-1 rounded-[10px] border border-line bg-surface-sunken p-1"
				>
					<button
						v-for="opt in notifyOptions"
						:key="opt.value"
						type="button"
						class="flex items-center justify-center gap-1.5 rounded-[7px] py-2.5 text-xs font-medium transition"
						:class="
							draft.notify === opt.value
								? 'bg-surface text-ink'
								: 'text-ink-muted hover:bg-surface-hover'
						"
						@click="set('notify', opt.value)"
					>
						<component :is="opt.icon" :size="13" />
						{{ opt.label }}
					</button>
				</div>
			</div>

			<div class="mb-5">
				<SettingLabel>End-of-interval sound</SettingLabel>
				<div class="rounded-[10px] border border-line bg-surface-sunken p-1.5">
					<div class="grid grid-cols-4 gap-1">
						<button
							v-for="s in sounds"
							:key="s.id"
							type="button"
							class="rounded-md py-2 text-xs transition"
							:class="
								draft.sound === s.id
									? 'bg-status-fix-bg font-semibold text-status-fix-fg'
									: 'text-ink-muted hover:bg-surface-hover'
							"
							@click="pickSound(s.id)"
						>
							{{ s.label }}
						</button>
					</div>
					<div
						class="mt-2 flex items-center gap-2.5 border-t border-line px-2.5 pt-2 pb-1"
					>
						<component
							:is="draft.volume === 0 ? VolumeX : Volume2"
							:size="14"
							class="text-ink-subtle"
						/>
						<input
							type="range"
							min="0"
							max="100"
							:value="draft.volume"
							class="h-1 flex-1 accent-[#e85a4f]"
							@input="
								set(
									'volume',
									parseInt(($event.target as HTMLInputElement).value, 10),
								)
							"
						/>
						<span
							class="min-w-[32px] text-right text-[11px] text-ink-muted tabular-nums"
						>
							{{ draft.volume }}%
						</span>
					</div>
				</div>
			</div>

			<p
				v-if="saveError"
				role="alert"
				class="mb-3 text-[13px] text-status-fix-fg"
			>
				{{ saveError }}
			</p>

			<button
				type="button"
				class="w-full rounded-[10px] bg-brand py-3.5 text-sm font-semibold text-on-accent transition hover:bg-brand-hover disabled:cursor-not-allowed disabled:opacity-60"
				:disabled="saving"
				@click="handleSave"
			>
				{{ saving ? 'Saving…' : 'Save' }}
			</button>
		</div>
	</div>
</template>

<script lang="ts">
	import {
		PomodoroSettings,
		savePomodoroSettings,
	} from '@/actions/tmgr/pomodoro';
	import { useDialogFocus } from '@/composable/useDialogFocus';
	import { useModalEscHandler } from '@/composable/useModalEscHandler';
	import { playPomodoroSound } from '@/utils/pomodoroSound';
	import { Bell, Eye, Minus, Plus, Volume2, VolumeX, X } from 'lucide-vue-next';
	import {
		defineComponent,
		h,
		onBeforeUnmount,
		PropType,
		ref,
		watch,
	} from 'vue';
	import { useStore } from 'vuex';

	const SettingLabel = defineComponent({
		name: 'PomoSettingLabel',
		setup(_, { slots }) {
			return () =>
				h(
					'div',
					{
						class:
							'mb-2 text-[10px] font-bold uppercase tracking-[.12em] text-ink-muted',
					},
					slots.default ? slots.default() : [],
				);
		},
	});

	const TimeField = defineComponent({
		name: 'PomoTimeField',
		props: {
			modelValue: { type: Number, required: true },
			accent: { type: String, default: '#e85a4f' },
		},
		emits: ['update:modelValue'],
		setup(props, { emit }) {
			const editing = ref(false);
			const buf = ref(String(props.modelValue).padStart(2, '0'));
			watch(
				() => props.modelValue,
				(v) => {
					buf.value = String(v).padStart(2, '0');
				},
			);
			const commit = () => {
				const n = parseInt(buf.value.replace(/\D/g, ''), 10);
				if (Number.isFinite(n))
					emit('update:modelValue', Math.max(1, Math.min(180, n)));
				editing.value = false;
			};
			return { editing, buf, commit, props };
		},
		render() {
			const mm = String(Math.max(0, this.modelValue)).padStart(2, '0');
			const inner = this.editing
				? h('input', {
						value: this.buf,
						onVnodeMounted: (vnode: { el: unknown }) =>
							(vnode.el as HTMLInputElement).focus(),
						'data-esc-blur': '',
						onInput: (e: Event) =>
							(this.buf = (e.target as HTMLInputElement).value),
						onBlur: this.commit,
						onKeydown: (e: KeyboardEvent) => {
							if (e.key === 'Enter') this.commit();
							if (e.key === 'Escape') this.editing = false;
						},
						class:
							'flex-1 bg-transparent border-0 outline-hidden p-0 text-ink tracking-wide',
						style: {
							font: '600 18px/1 "JetBrains Mono", "SF Mono", ui-monospace, monospace',
						},
					})
				: h(
						'span',
						{
							class: 'flex-1 text-ink tracking-wide',
							style: {
								font: '600 18px/1 "JetBrains Mono", "SF Mono", ui-monospace, monospace',
							},
						},
						[mm, h('span', { class: 'text-ink-subtle' }, ':00')],
					);
			return h(
				'div',
				{
					class:
						'relative flex h-11 cursor-text items-center rounded-lg border border-line bg-surface-sunken px-3.5',
					onClick: () => (this.editing = true),
				},
				[
					inner,
					h(
						'span',
						{ class: 'mr-2.5 text-[11px] font-medium text-ink-muted' },
						'min',
					),
					h('span', {
						class: 'inline-block h-1.5 w-1.5 rounded-full',
						style: {
							background: this.accent,
							boxShadow: `0 0 6px ${this.accent}`,
						},
					}),
				],
			);
		},
	});

	const Stepper = defineComponent({
		name: 'PomoStepper',
		props: {
			modelValue: { type: Number, required: true },
			min: { type: Number, default: 0 },
			max: { type: Number, default: 99 },
			suffix: { type: String, default: '' },
		},
		emits: ['update:modelValue'],
		render() {
			return h(
				'div',
				{
					class:
						'flex h-11 items-center rounded-lg border border-line bg-surface-sunken py-0 pl-3.5 pr-1',
				},
				[
					h(
						'span',
						{
							class: 'min-w-[28px] tracking-wide text-ink',
							style: {
								font: '600 18px/1 "JetBrains Mono", "SF Mono", ui-monospace, monospace',
							},
						},
						String(this.modelValue),
					),
					h(
						'span',
						{ class: 'ml-2 flex-1 text-[11px] font-medium text-ink-muted' },
						this.suffix,
					),
					h(
						'button',
						{
							type: 'button',
							class:
								'flex h-7 w-7 items-center justify-center rounded-md hover:bg-surface-hover',
							onClick: () =>
								this.$emit(
									'update:modelValue',
									Math.max(this.min, this.modelValue - 1),
								),
						},
						[h(Minus, { size: 13, class: 'text-ink-subtle' })],
					),
					h(
						'button',
						{
							type: 'button',
							class:
								'flex h-7 w-7 items-center justify-center rounded-md hover:bg-surface-hover',
							onClick: () =>
								this.$emit(
									'update:modelValue',
									Math.min(this.max, this.modelValue + 1),
								),
						},
						[h(Plus, { size: 13, class: 'text-ink-subtle' })],
					),
				],
			);
		},
	});

	const RowToggle = defineComponent({
		name: 'PomoRowToggle',
		props: {
			title: { type: String, required: true },
			subtitle: { type: String, default: '' },
			modelValue: { type: Boolean, required: true },
		},
		emits: ['update:modelValue'],
		render() {
			return h('div', { class: 'flex items-center gap-3 px-3.5 py-3' }, [
				h('div', { class: 'min-w-0 flex-1' }, [
					h('div', { class: 'text-[13px] font-medium text-ink' }, this.title),
					this.subtitle
						? h(
								'div',
								{ class: 'mt-0.5 text-[11px] text-ink-muted' },
								this.subtitle,
							)
						: null,
				]),
				h(
					'button',
					{
						type: 'button',
						class: [
							'relative h-5 w-9 rounded-full border-0 p-0 transition',
							this.modelValue ? 'bg-status-done' : 'bg-ink-faint',
						],
						onClick: () => this.$emit('update:modelValue', !this.modelValue),
					},
					[
						h('span', {
							class:
								'absolute top-0.5 h-4 w-4 rounded-full bg-white shadow transition-all',
							style: { left: this.modelValue ? '18px' : '2px' },
						}),
					],
				),
			]);
		},
	});

	export default defineComponent({
		name: 'PomodoroSettingsModal',
		components: { SettingLabel, TimeField, Stepper, RowToggle, X },
		props: {
			open: { type: Boolean, default: false },
			value: {
				type: Object as PropType<PomodoroSettings>,
				required: true,
			},
		},
		emits: ['close', 'save'],
		setup(props, { emit }) {
			const draft = ref<PomodoroSettings>({ ...props.value });
			const saving = ref(false);
			const saveError = ref('');
			const store = useStore();
			const modalId = `pomodoro-settings-${Date.now()}-${Math.random()}`;
			const dialogEl = ref<HTMLElement | null>(null);
			const dialogFocus = useDialogFocus(
				modalId,
				dialogEl,
				'Pomodoro settings',
			);
			const { registerModal, unregisterModal } = useModalEscHandler();
			let tracked = false;

			const untrack = () => {
				if (!tracked) return;
				tracked = false;
				dialogFocus.deactivate();
				unregisterModal(modalId);
				store.commit('removeModalFromStack', modalId);
			};

			watch(
				() => props.open,
				(isOpen) => {
					if (!isOpen) {
						untrack();
						return;
					}
					draft.value = { ...props.value };
					saveError.value = '';
					tracked = true;
					store.commit('pushModalToStack', modalId);
					registerModal(modalId, () => emit('close'));
					dialogFocus.activate();
				},
				{ flush: 'post' },
			);

			onBeforeUnmount(untrack);

			const set = <K extends keyof PomodoroSettings>(
				key: K,
				val: PomodoroSettings[K],
			) => {
				draft.value = { ...draft.value, [key]: val };
			};

			const pickSound = (id: string) => {
				set('sound', id);
				playPomodoroSound(id, draft.value.volume);
			};

			const handleSave = async () => {
				saving.value = true;
				saveError.value = '';
				try {
					await savePomodoroSettings(draft.value);
				} catch {
					saveError.value = "Couldn't save settings. Try again.";
					return;
				} finally {
					saving.value = false;
				}
				emit('save', { ...draft.value });
				emit('close');
			};

			return {
				draft,
				saving,
				saveError,
				dialogEl,
				set,
				pickSound,
				handleSave,
				notifyOptions: [
					{ value: 'silent' as const, label: 'Silent', icon: Eye },
					{ value: 'system' as const, label: 'System', icon: Bell },
					{ value: 'both' as const, label: 'Both', icon: Volume2 },
				],
				sounds: [
					{ id: 'bell', label: 'Bell' },
					{ id: 'chime', label: 'Chime' },
					{ id: 'wood', label: 'Wood' },
					{ id: 'digital', label: 'Digital' },
				],
				X,
				Volume2,
				VolumeX,
			};
		},
	});
</script>
