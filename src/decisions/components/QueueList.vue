<template>
	<nav class="sidebar">
		<div class="ws">
			<div class="logo">⚡</div>
			<b>Decisions</b>
			<span v-if="progress.pending" class="badge">{{ progress.pending }}</span>
		</div>
		<div class="sect">
			Queue<span>{{ progress.pending }} of {{ progress.total }}</span>
		</div>
		<div class="prog"><i :style="{ width: progress.percent + '%' }"></i></div>
		<div v-if="groups.length === 0" class="empty-queue">
			Nothing in the queue.
		</div>
		<template v-for="group in groups" :key="group.key">
			<div class="group" :title="group.agent">
				{{ group.label }} · {{ group.agent }}
			</div>
			<button
				v-for="card in group.cards"
				:key="card.id"
				type="button"
				class="item"
				:class="{ active: card.id === selectedId, done: isDone(card) }"
				@click="$emit('select', card.id)"
			>
				<div class="row">
					<span class="tag" :class="'t-' + card.kind">{{
						KIND_LABELS[card.kind]
					}}</span>
					<span v-if="card.state === 'deferred'" class="k">deferred</span>
					<span
						v-if="card.blocking && !isDone(card)"
						class="block"
						title="An agent is waiting"
					></span>
				</div>
				<span class="t">{{ card.title }}</span>
			</button>
		</template>
	</nav>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import { KIND_LABELS } from '../format';
	import { isPending, type QueueGroup, type QueueProgress } from '../queue';
	import type { CardView } from '../types';

	export default defineComponent({
		name: 'QueueList',
		props: {
			groups: { type: Array as PropType<QueueGroup[]>, required: true },
			selectedId: { type: String as PropType<string | null>, default: null },
			progress: { type: Object as PropType<QueueProgress>, required: true },
		},
		emits: ['select'],
		data() {
			return { KIND_LABELS };
		},
		methods: {
			isDone(card: CardView): boolean {
				return !isPending(card);
			},
		},
	});
</script>
