<template>
	<DaySummaryModal
		v-if="request"
		:key="request.getTime()"
		:day="isToday ? undefined : request"
		@close="request = null"
	/>
</template>

<script setup lang="ts">
	import DaySummaryModal from '@/components/DaySummaryModal.vue';
	import { daySummaryRequest as request } from '@/local/daySummaryScheduler';
	import { localDay } from '@/local/daySummarySchedule';
	import { computed } from 'vue';

	const isToday = computed(
		() => !!request.value && localDay(request.value) === localDay(new Date()),
	);
</script>
