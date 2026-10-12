<template>
	<div class="flex flex-col gap-2 sm:relative sm:block">
		<pre
			:aria-label="label"
			class="overflow-x-auto whitespace-pre rounded-md border border-line bg-surface-sunken px-3 py-3 font-mono sm:pr-24 text-xs text-ink dark:border-gray-700 dark:bg-gray-950"
			>{{ display ?? code }}</pre
		>
		<button
			type="button"
			class="inline-flex self-end sm:absolute sm:right-2 sm:top-2 min-h-[44px] items-center rounded-md border border-line bg-surface px-3 text-sm font-medium text-ink hover:bg-surface-sunken focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand sm:min-h-[32px] dark:border-gray-700 dark:bg-gray-900"
			:disabled="!code"
			@click="copy"
		>
			{{ copied ? 'Copied' : 'Copy' }}
		</button>
		<p v-if="failed" class="mt-1 text-xs text-red-600 dark:text-red-400" role="alert">
			Could not copy. Select the text and copy it by hand.
		</p>
	</div>
</template>

<script lang="ts">
import { defineComponent, onBeforeUnmount, ref } from 'vue';
import { tourBus } from '@/tours/events';

export default defineComponent({
	name: 'CopyCommand',
	props: {
		code: { type: String, required: true },
		display: { type: String, default: undefined },
		label: { type: String, default: 'Command' },
		client: { type: String, default: '' },
		emitEvent: { type: Boolean, default: true },
	},
	emits: ['copied'],
	setup(props, { emit }) {
		const copied = ref(false);
		const failed = ref(false);
		let timer: ReturnType<typeof setTimeout> | null = null;

		const copy = async () => {
			failed.value = false;
			try {
				await navigator.clipboard.writeText(props.code);
			} catch {
				failed.value = true;
				return;
			}
			copied.value = true;
			if (timer) clearTimeout(timer);
			timer = setTimeout(() => (copied.value = false), 2000);
			if (props.emitEvent)
				tourBus.emit('connect.command_copied', { client: props.client });
			emit('copied');
		};

		onBeforeUnmount(() => {
			if (timer) clearTimeout(timer);
		});

		return { copied, failed, copy };
	},
});
</script>
