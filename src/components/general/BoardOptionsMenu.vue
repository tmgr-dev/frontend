<template>
	<div class="relative" ref="$wrapper">
		<svg
			class="text-grey-dark mr-3 w-6 fill-current md:mr-1"
			viewBox="0 0 20 20"
			xmlns="http://www.w3.org/2000/svg"
			@click="opened = !opened"
		>
			<path
				d="M4 12a2 2 0 1 1 0-4 2 2 0 0 1 0 4zm6 0a2 2 0 1 1 0-4 2 2 0 0 1 0 4zm6 0a2 2 0 1 1 0-4 2 2 0 0 1 0 4z"
			/>
		</svg>

		<div
			v-if="opened"
			class="absolute right-0 z-50 mt-2 w-40 overflow-hidden rounded-md bg-white shadow-lg"
		>
			<a
				v-for="action in props.actions"
				:href="action.link || '#'"
				class="flex items-center border-b px-3 py-3 last:border-b-0 hover:bg-gray-100"
				@click.prevent="
					() => {
						action.click();
						opened = false;
					}
				"
			>
				<span class="text-sm text-gray-600">{{ action.label }}</span>
			</a>
		</div>
	</div>
</template>

<script lang="ts" setup>
	import { ref, defineProps, Ref, onMounted } from 'vue';

	type action = {
		label: string;
		link?: string;
		click: () => void;
	};

	interface Props {
		actions: action[];
	}

	const props = defineProps<Props>();

	const $wrapper: Ref<HTMLDivElement | null> = ref(null);
	const opened = ref(false);

	onMounted(async () => {
		document.addEventListener('click', (e: MouseEvent) => {
			if (!$wrapper.value?.contains(e.target as Node)) {
				opened.value = false;
			}
		});
	});
</script>
