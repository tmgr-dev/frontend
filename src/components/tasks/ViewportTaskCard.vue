<script setup lang="ts">
	import {
		nextTick,
		onBeforeUnmount,
		onMounted,
		ref,
		useAttrs,
		watch,
	} from 'vue';
	import TaskBoardCard from './TaskBoardCard.vue';

	defineOptions({ inheritAttrs: false });
	const props = defineProps<{
		task: any;
		statuses: any[];
		enabled?: boolean;
	}>();
	const attrs = useAttrs();
	const root = ref<HTMLElement>();
	const visible = ref(!props.enabled);
	const height = ref(140);
	let observer: IntersectionObserver | undefined;
	let resize: ResizeObserver | undefined;
	let pinned = false;
	let intersects = true;
	const update = () => {
		visible.value =
			!props.enabled ||
			intersects ||
			pinned ||
			!!root.value?.contains(document.activeElement);
	};
	watch(
		() => props.enabled,
		() => update(),
	);
	const release = () => {
		pinned = false;
		update();
	};
	const focus = async () => {
		visible.value = true;
		await nextTick();
		root.value?.querySelector<HTMLElement>('button')?.focus();
	};
	onMounted(() => {
		if (!root.value || typeof IntersectionObserver === 'undefined') {
			visible.value = true;
			return;
		}
		observer = new IntersectionObserver(
			([entry]) => {
				intersects = entry.isIntersecting;
				update();
			},
			{ rootMargin: '600px 300px' },
		);
		observer.observe(root.value);
		resize = new ResizeObserver(([entry]) => {
			if (visible.value && entry.contentRect.height > 0)
				height.value = entry.contentRect.height;
		});
		resize.observe(root.value);
		window.addEventListener('pointerup', release);
	});
	onBeforeUnmount(() => {
		observer?.disconnect();
		resize?.disconnect();
		window.removeEventListener('pointerup', release);
	});
</script>
<template>
	<div
		ref="root"
		:class="attrs.class"
		:data-task-id="task.id"
		:data-draggable="attrs['data-draggable']"
		:style="visible ? undefined : { height: `${height}px` }"
		:tabindex="visible ? undefined : 0"
		:aria-label="visible ? undefined : task.title"
		@focus.self="focus"
		@pointerdown="pinned = true"
	>
		<TaskBoardCard
			v-if="visible"
			v-bind="{
				...attrs,
				class: undefined,
				'data-draggable': undefined,
				'data-task-id': undefined,
			}"
			:task="task"
			:statuses="statuses"
		/>
		<div
			v-else
			aria-hidden="true"
			class="h-full rounded-card border border-line bg-surface"
		/>
	</div>
</template>
