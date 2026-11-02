<template>
	<div ref="root" class="h-px w-full shrink-0" aria-hidden="true"></div>
</template>

<script lang="ts">
	import { defineComponent, onBeforeUnmount, onMounted, ref } from 'vue';

	export default defineComponent({
		name: 'ColumnSentinel',
		emits: ['visible'],
		setup(_, { emit }) {
			const root = ref<HTMLElement>();
			let observer: IntersectionObserver | undefined;
			onMounted(() => {
				observer = new IntersectionObserver(
					(entries) => {
						if (entries.some((entry) => entry.isIntersecting)) emit('visible');
					},
					{
						root: root.value?.closest('.board-card-draggable') ?? null,
						rootMargin: '300px',
					},
				);
				if (root.value) observer.observe(root.value);
			});
			onBeforeUnmount(() => observer?.disconnect());
			return { root };
		},
	});
</script>
