<script setup lang="ts">
	import { useVoiceInput } from '@/composable/useVoiceInput';
	import { resolveFocusTarget } from '@/utils/focusTarget';
	import { formatClock, insertAtCursor, MAX_SECONDS } from '@/utils/voiceInput';
	import { Loader2, Mic, Square, X } from 'lucide-vue-next';
	import { computed, nextTick, onBeforeUnmount, watch } from 'vue';

	const props = defineProps<{
		modelValue: string;
		target?: unknown;
		disabled?: boolean;
	}>();

	const emit = defineEmits<{
		(e: 'update:modelValue', value: string): void;
	}>();

	type Field = HTMLInputElement | HTMLTextAreaElement;

	function field(): Field | null {
		return resolveFocusTarget(props.target) as Field | null;
	}

	function insert(text: string): void {
		const el = field();
		const { value, caret } = insertAtCursor(
			props.modelValue,
			text,
			el?.selectionStart,
			el?.selectionEnd,
			el?.tagName !== 'TEXTAREA',
		);
		emit('update:modelValue', value);
		nextTick(() => {
			el?.focus();
			el?.setSelectionRange?.(caret, caret);
		});
	}

	const { state, supported, toggle, cancel, clearError } =
		useVoiceInput(insert);

	const recording = computed(() => state.value.status === 'recording');
	const working = computed(() =>
		['requesting', 'transcribing'].includes(state.value.status),
	);
	const unavailable = computed(
		() => !supported || state.value.blocked || !!props.disabled,
	);
	const title = computed(() => {
		if (!supported) return 'Voice input is not supported here';
		if (state.value.blocked && state.value.error) return state.value.error;
		if (recording.value) return 'Stop and transcribe';
		return 'Dictate';
	});

	watch(
		() => props.disabled,
		(off) => {
			if (off && state.value.status !== 'idle') cancel();
		},
	);

	let errorTimer: ReturnType<typeof setTimeout> | null = null;
	watch(
		() => state.value.error,
		(message) => {
			if (errorTimer) clearTimeout(errorTimer);
			errorTimer = null;
			if (message && !state.value.blocked)
				errorTimer = setTimeout(clearError, 6000);
		},
	);
	onBeforeUnmount(() => {
		if (errorTimer) clearTimeout(errorTimer);
	});

	defineExpose({ cancel });
</script>

<template>
	<span class="relative inline-flex items-center gap-1" @mousedown.prevent>
		<template v-if="recording">
			<span
				class="text-xs tabular-nums text-ink-subtle"
				:class="{ 'text-red-500': state.seconds >= MAX_SECONDS - 10 }"
				aria-live="off"
				>{{ formatClock(state.seconds) }}</span
			>
			<button
				type="button"
				class="flex h-7 w-7 items-center justify-center rounded-pill text-ink-subtle hover:bg-surface-hover hover:text-ink"
				aria-label="Cancel recording"
				title="Cancel recording"
				@click="cancel"
			>
				<X class="h-4 w-4" />
			</button>
		</template>
		<button
			type="button"
			class="flex h-7 w-7 items-center justify-center rounded-pill transition disabled:cursor-not-allowed disabled:opacity-40"
			:class="
				recording
					? 'bg-red-500 text-white hover:bg-red-600'
					: 'text-ink-subtle hover:bg-surface-hover hover:text-ink'
			"
			:disabled="unavailable || working"
			:aria-label="recording ? 'Stop recording' : 'Start voice input'"
			:aria-pressed="recording"
			:title="title"
			@click="toggle"
		>
			<Loader2 v-if="working" class="h-4 w-4 animate-spin" />
			<Square v-else-if="recording" class="h-3.5 w-3.5 fill-current" />
			<Mic v-else class="h-4 w-4" />
		</button>
		<span
			v-if="state.error"
			role="alert"
			class="absolute bottom-full right-0 z-10 mb-2 w-max max-w-[16rem] rounded-md border border-line bg-surface px-2 py-1 text-xs text-red-500 shadow-md"
			>{{ state.error }}</span
		>
	</span>
</template>
