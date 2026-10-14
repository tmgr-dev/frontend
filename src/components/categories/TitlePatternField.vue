<template>
	<div class="flex min-w-0 flex-col gap-2">
		<input
			ref="input"
			:value="modelValue"
			type="text"
			autocomplete="off"
			spellcheck="false"
			:aria-label="label"
			:aria-invalid="literal || undefined"
			class="h-9 w-full rounded-lg border bg-surface-base px-2.5 font-mono text-[13px] text-ink outline-none transition-colors placeholder:text-ink-faint focus:!border-primary"
			:class="literal ? 'border-status-progress' : 'border-line-strong'"
			@input="$emit('update:modelValue', $event.target.value)"
		/>
		<div
			v-if="literal"
			class="flex flex-wrap items-center justify-between gap-3 rounded-lg bg-status-progress-bg px-3 py-2.5 text-xs leading-relaxed text-ink dark:bg-[oklch(0.3_0.05_75/0.35)] lg:flex-nowrap"
		>
			<span class="min-w-0 flex-1">
				<b class="font-semibold text-status-progress">No placeholders.</b>
				Every new task will literally start with “<span
					class="break-all font-mono"
					>{{ modelValue }}</span
				>”.
			</span>
			<Button
				type="button"
				variant="outline"
				size="sm"
				class="h-7 max-w-full shrink-0 border-line-strong bg-surface text-xs text-ink"
				@click="$emit('update:modelValue', fix)"
			>
				Use <span class="truncate font-mono text-[11px]">{{ fix }}</span>
			</Button>
		</div>
		<div class="flex min-w-0 items-baseline gap-2 text-xs text-ink-subtle">
			<span class="shrink-0">Preview</span>
			<span class="truncate font-mono text-ink">{{ preview }}</span>
		</div>
		<div class="flex flex-wrap gap-1.5 max-lg:hidden">
			<button
				v-for="chip in chips"
				:key="chip.token"
				type="button"
				class="inline-flex h-6 items-center rounded-full border border-dashed border-line-strong px-2 text-xs font-medium text-ink-muted transition-colors hover:bg-surface-hover hover:text-ink"
				@mousedown.prevent
				@click="insert(chip.token)"
			>
				+ {{ chip.label }}
			</button>
		</div>
	</div>
</template>

<script>
	import { Button } from '@/components/ui/button';
	import {
		composeTitle,
		insertAt,
		isLiteralPattern,
		literalPatternFix,
		renderTitlePrefix,
	} from '@/utils/categorySettingsPage';
	import { computed, defineComponent, nextTick, ref } from 'vue';

	const CHIPS = [
		{ label: 'category number', token: '{index#category}' },
		{ label: 'workspace number', token: '{index#workspace}' },
		{ label: 'date', token: '{dts#dd.mm.yyyy#dte}' },
		{ label: 'weekday', token: '{dts#ddd#dte}' },
	];

	export default defineComponent({
		name: 'TitlePatternField',
		components: { Button },
		props: {
			modelValue: { type: [String, Number], default: '' },
			label: { type: String, default: 'Task title pattern' },
		},
		emits: ['update:modelValue'],
		setup(props, { emit }) {
			const input = ref(null);
			const value = computed(() => String(props.modelValue ?? ''));
			const literal = computed(() => isLiteralPattern(value.value));
			const fix = computed(() => literalPatternFix(value.value));
			const preview = computed(() =>
				composeTitle(renderTitlePrefix(value.value), 'Task title'),
			);

			const insert = async (token) => {
				const el = input.value;
				const start = el?.selectionStart ?? value.value.length;
				const end = el?.selectionEnd ?? start;
				const next = insertAt(value.value, token, start, end);
				emit('update:modelValue', next.value);
				await nextTick();
				el?.focus();
				el?.setSelectionRange(next.caret, next.caret);
			};

			return { input, literal, fix, preview, chips: CHIPS, insert };
		},
	});
</script>
