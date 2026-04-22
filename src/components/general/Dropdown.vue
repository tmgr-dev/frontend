<template>
	<Menu as="div" class="relative inline-block text-left">
		<div>
			<slot name="toggle">
				<MenuButton
					class="flex items-center text-ink-subtle transition-colors hover:text-ink focus:outline-none"
				>
					<span class="sr-only">Open options</span>
					<EllipsisVerticalIcon class="h-5 w-5" aria-hidden="true" />
				</MenuButton>
			</slot>
		</div>

		<Transition
			enter-active-class="transition ease-out duration-100"
			enter-from-class="transform opacity-0 scale-95"
			enter-to-class="transform opacity-100 scale-100"
			leave-active-class="transition ease-in duration-75"
			leave-from-class="transform opacity-100 scale-100"
			leave-to-class="transform opacity-0 scale-95"
		>
			<MenuItems
				class="absolute right-0 z-20 mt-2 w-56 origin-top-right overflow-hidden rounded-card border border-line bg-surface shadow-tmgr-md focus:outline-none"
			>
				<div class="py-1">
					<slot />
				</div>
			</MenuItems>
		</Transition>
	</Menu>
</template>

<script setup>
	import { Menu, MenuButton, MenuItems } from '@headlessui/vue';
	import { EllipsisVerticalIcon } from '@heroicons/vue/20/solid';
</script>
