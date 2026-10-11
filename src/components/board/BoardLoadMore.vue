<template>
	<div ref="root" class="board-load-more mx-1 mt-1">
		<button
			type="button"
			:disabled="loading"
			class="flex w-full items-center justify-center rounded-md px-2 py-1.5 text-xs font-medium text-ink-muted transition-colors hover:bg-surface-hover hover:text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
			@click="$emit('more')"
		>
			{{ loading ? 'Loading…' : 'Load more' }}
		</button>
	</div>
</template>

<script setup lang="ts">
	import { onBeforeUnmount, onMounted, ref, watch } from 'vue';

	const props = defineProps<{ loading: boolean }>();
	const emit = defineEmits<{ (e: 'more'): void }>();
	const root = ref<HTMLElement | null>(null);
	let observer: IntersectionObserver | null = null;

	const observe = () => {
		if (!observer || !root.value) return;
		observer.unobserve(root.value);
		observer.observe(root.value);
	};

	onMounted(() => {
		if (!root.value || typeof IntersectionObserver === 'undefined') return;
		observer = new IntersectionObserver(
			(entries) => {
				if (entries.some((entry) => entry.isIntersecting) && !props.loading)
					emit('more');
			},
			{
				root: root.value.closest('.board-card-draggable'),
				rootMargin: '0px 0px 300px 0px',
			},
		);
		observe();
	});

	watch(
		() => props.loading,
		(loading) => {
			if (!loading) observe();
		},
	);

	onBeforeUnmount(() => observer?.disconnect());
</script>
