<script setup lang="ts">
	import { cn } from '@/utils';
	import { X } from 'lucide-vue-next';
	import { ToastClose, type ToastCloseProps } from 'radix-vue';
	import { computed, type HTMLAttributes } from 'vue';

	const props = defineProps<
		ToastCloseProps & {
			class?: HTMLAttributes['class'];
		}
	>();

	const delegatedProps = computed(() => {
		const { class: _, ...delegated } = props;

		return delegated;
	});
</script>

<template>
	<ToastClose
		v-bind="delegatedProps"
		:class="
			cn(
				'absolute top-2 right-2 rounded-md p-1 text-foreground/50 opacity-0 transition-opacity group-hover:opacity-100 group-[.destructive]:text-red-300 hover:text-foreground group-[.destructive]:hover:text-red-50 focus:opacity-100',
				props.class,
			)
		"
	>
		<X class="h-4 w-4" />
	</ToastClose>
</template>
