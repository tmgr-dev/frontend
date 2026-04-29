<template>
	<svg
		:width="size"
		:height="size"
		viewBox="0 0 24 24"
		fill="none"
		:stroke="stroke"
		:stroke-width="sw"
		stroke-linecap="round"
		stroke-linejoin="round"
		v-html="path"
	/>
</template>

<script setup lang="ts">
	import { computed } from 'vue';

	interface Props {
		name: string;
		size?: number;
		stroke?: string;
		sw?: number;
	}

	const props = withDefaults(defineProps<Props>(), {
		size: 16,
		stroke: 'currentColor',
		sw: 1.6,
	});

	const ICONS: Record<string, string> = {
		list: '<path d="M8 6h12M8 12h12M8 18h12"/><circle cx="4" cy="6" r="1" fill="currentColor"/><circle cx="4" cy="12" r="1" fill="currentColor"/><circle cx="4" cy="18" r="1" fill="currentColor"/>',
		day: '<rect x="4" y="5" width="16" height="16" rx="2"/><path d="M4 10h16"/><path d="M9 14h6M9 17h4"/>',
		week: '<rect x="3" y="5" width="18" height="16" rx="2"/><path d="M3 9h18M9 9v12M15 9v12"/>',
		month: '<rect x="3" y="5" width="18" height="16" rx="2"/><path d="M3 9h18M8 5V3M16 5V3M8 13h.01M12 13h.01M16 13h.01M8 17h.01M12 17h.01"/>',
		year: '<rect x="3" y="4" width="6" height="6" rx="1"/><rect x="11" y="4" width="6" height="6" rx="1"/><rect x="3" y="12" width="6" height="6" rx="1"/><rect x="11" y="12" width="6" height="6" rx="1" fill="currentColor"/>',
		plus: '<path d="M12 5v14M5 12h14"/>',
		check: '<path d="M5 12l4 4 10-10"/>',
		trash: '<path d="M5 7h14M9 7V5h6v2M7 7l1 12h8l1-12"/>',
		'chev-l': '<path d="M14 6l-6 6 6 6"/>',
		'chev-r': '<path d="M10 6l6 6-6 6"/>',
		repeat: '<path d="M17 3l3 3-3 3"/><path d="M20 6H8a4 4 0 0 0-4 4v1"/><path d="M7 21l-3-3 3-3"/><path d="M4 18h12a4 4 0 0 0 4-4v-1"/>',
		clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
		bell: '<path d="M6 16V11a6 6 0 1 1 12 0v5l1.5 2h-15z"/><path d="M10 21a2 2 0 0 0 4 0"/>',
		close: '<path d="M6 6l12 12M6 18L18 6"/>',
		pencil: '<path d="M4 20h4l10.5-10.5a2.121 2.121 0 0 0-3-3L5 17v3z"/><path d="M13.5 6.5l3 3"/>',
		hourglass: '<path d="M6 3h12M6 21h12M7 3v3a5 5 0 0 0 5 5 5 5 0 0 0 5-5V3M7 21v-3a5 5 0 0 1 5-5 5 5 0 0 1 5 5v3"/>',
		download: '<path d="M12 3v12m0 0l-4-4m4 4l4-4M4 21h16"/>',
		upload: '<path d="M12 21V9m0 0l-4 4m4-4l4 4M4 3h16"/>',
		archive: '<rect x="3" y="4" width="18" height="4" rx="1"/><path d="M5 8v11a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V8"/><path d="M10 12h4"/>',
	};

	const path = computed(() => ICONS[props.name] || '');
</script>
