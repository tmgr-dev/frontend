<template>
	<span
		v-if="count > 0"
		class="inline-flex shrink-0 items-center gap-0.5 tabular-nums no-underline"
		data-testid="routine-comment-count"
	>
		<MessageCircle class="size-2.5" aria-hidden="true" />
		<span aria-hidden="true">{{ count }}</span>
		<span class="sr-only">{{
			$t('dailyRoutine.comments.count', { n: count })
		}}</span>
	</span>
</template>

<script setup lang="ts">
	import { MessageCircle } from 'lucide-vue-next';

	defineProps<{ count: number }>();
</script>
