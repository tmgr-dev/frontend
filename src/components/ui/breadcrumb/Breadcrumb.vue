<script lang="ts" setup>
	import type { HTMLAttributes } from 'vue';

	const props = defineProps<{
		class?: HTMLAttributes['class'];
	}>();
</script>

<template>
	<nav :aria-label="$t('general.breadcrumbs.label')" :class="props.class">
		<slot />
	</nav>
</template>
