<script setup lang="ts">
	import { cn } from '@/utils';
	import type { PrimitiveProps } from 'radix-vue';
	import { Primitive } from 'radix-vue';
	import type { HTMLAttributes } from 'vue';

	const props = defineProps<
		PrimitiveProps & {
			class?: HTMLAttributes['class'];
		}
	>();
</script>

<template>
	<Primitive
		data-sidebar="group-action"
		:as="as"
		:as-child="asChild"
		:class="
			cn(
				'absolute top-3.5 right-3 flex aspect-square w-5 items-center justify-center rounded-md p-0 text-sidebar-foreground outline-hidden transition-transform hover:bg-sidebar-accent hover:text-sidebar-accent-foreground [&>svg]:size-4 [&>svg]:shrink-0',
				// Increases the hit area of the button on mobile.
				'after:absolute after:-inset-2 after:md:hidden',
				'group-data-[collapsible=icon]:hidden',
				props.class,
			)
		"
	>
		<slot />
	</Primitive>
</template>
