<template>
	<div class="relative">
		<pre
			:aria-label="label"
			class="overflow-x-auto whitespace-pre rounded-md border border-border bg-muted px-3 py-2 pr-20 font-mono text-xs text-ink"
			>{{ code }}</pre
		>
		<Button
			type="button"
			variant="outline"
			size="sm"
			class="absolute right-2 top-2 h-7 px-2 text-xs"
			@click="copy"
		>
			{{ copied ? 'Copied' : 'Copy' }}
		</Button>
	</div>
</template>

<script setup lang="ts">
	import { Button } from '@/components/ui/button';
	import { toast } from '@/components/ui/toast';
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
