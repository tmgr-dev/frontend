<template>
	<div :class="SNIPPET_ROW_CLASS">
		<pre
			:aria-label="label"
			:class="[
				snippetTextClass(false),
				'rounded-md border border-border bg-muted px-3 py-2 font-mono text-xs text-ink',
			]"
			data-testid="snippet-text"
			>{{ code }}</pre>
		<Button
			type="button"
			variant="outline"
			size="sm"
			:class="[SNIPPET_BUTTON_CLASS, 'h-11 px-2 text-xs sm:h-7']"
			data-testid="snippet-copy"
			:aria-label="`Copy ${label ?? 'code'}`"
			@click="copy"
		>
			{{ copied ? 'Copied' : 'Copy' }}
		</Button>
		<span class="sr-only" role="status" aria-live="polite">{{
			copied ? 'Copied' : ''
		}}</span>
	</div>
</template>

<script setup lang="ts">
	import { Button } from '@/components/ui/button';
	import { toast } from '@/components/ui/toast';
	import {
		SNIPPET_BUTTON_CLASS,
		SNIPPET_ROW_CLASS,
		snippetTextClass,
	} from '@/utils/snippetLayout';
	import { onBeforeUnmount, ref } from 'vue';

	const props = defineProps<{
		code: string;
		label?: string;
	}>();

	const copied = ref(false);
	let timer: ReturnType<typeof setTimeout> | null = null;

	async function copy() {
		try {
			await navigator.clipboard.writeText(props.code);
			copied.value = true;
			if (timer) clearTimeout(timer);
			timer = setTimeout(() => (copied.value = false), 2000);
		} catch (error) {
			toast({ title: 'Could not copy to clipboard', variant: 'destructive' });
		}
	}

	onBeforeUnmount(() => {
		if (timer) clearTimeout(timer);
	});
</script>
