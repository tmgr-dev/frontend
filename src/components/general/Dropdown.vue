<template>
	<Menu as="div" class="relative inline-block text-left">
		<div>
			<slot name="toggle">
				<MenuButton
					class="flex items-center rounded-full text-neutral-600 hover:opacity-80 focus:outline-none dark:text-white"
				>
					<span class="sr-only">Open options</span>
					<EllipsisHorizontalIcon class="h-6 w-6" aria-hidden="true" />
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
				class="absolute right-0 z-20 mt-2 w-56 origin-top-right rounded-md bg-white shadow-lg ring-1 ring-black ring-opacity-5 focus:outline-none"
			>
				<div class="py-1">
					<slot />
				</div>
			</MenuItems>
		</Transition>
	</Menu>
</template>

<script setup lang="ts">
	import { Menu, MenuButton, MenuItems } from '@headlessui/vue';
	import { EllipsisHorizontalIcon } from '@heroicons/vue/20/solid';
</script>
