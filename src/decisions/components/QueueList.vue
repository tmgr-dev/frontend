<template>
	<nav class="sidebar">
		<div class="ws">
			<div class="logo">⚡</div>
			<div class="ws-name">
				<b>Decisions</b>
				<small class="doom-sub">{{ subtitle }}</small>
			</div>
			<span v-if="progress.pending" class="badge">{{ progress.pending }}</span>
		</div>
		<div class="sect">
			Queue<span>{{ progress.pending }} of {{ progress.total }}</span>
		</div>
		<div class="prog"><i :style="{ width: progress.percent + '%' }"></i></div>
		<input
			type="text"
			autocomplete="off"
			class="queue-search"
			placeholder="Search queue"
			aria-label="Search queue"
			:value="query"
			@input="onInput"
			@keydown.esc="clearQuery"
		/>
		<div
			v-if="progress.total > 0 && progress.pending === 0"
			class="doom-complete"
		>
			<span class="grass" aria-hidden="true">🌱</span>
			Doom solving complete. Go touch grass.
		</div>
		<div v-if="groups.length === 0" class="empty-queue">
			{{
				query.trim() && progress.total > 0
					? 'No matches'
					: 'Nothing in the queue.'
			}}
		</div>
		<template v-for="group in groups" :key="group.key">
			<div class="group" :title="group.agent">
				{{ group.label }} · {{ group.agent }}
			</div>
			<div v-for="card in group.cards" :key="card.id" class="itemw">
				<button
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
						<span v-if="card.updated_ms" class="upd">updated</span>
						<span
							v-if="card.blocking && !isDone(card) && !card.session_gone"
							class="block"
							title="An agent is waiting"
						></span>
					</div>
					<span class="t">{{ card.title }}</span>
				</button>
				<button
					v-if="!isDone(card)"
					type="button"
					class="close"
					aria-label="Close card"
					title="Close card"
					@click.stop="$emit('dismiss', card.id)"
				>
					×
				</button>
			</div>
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
			query: { type: String, default: '' },
			subtitle: { type: String, default: 'doom solving' },
		},
		emits: ['select', 'dismiss', 'update:query'],
		data() {
			return { KIND_LABELS };
		},
		methods: {
			onInput(event: Event) {
				this.$emit('update:query', (event.target as HTMLInputElement).value);
			},
			clearQuery(event: KeyboardEvent) {
				if (!this.query || event.isComposing) return;
				event.preventDefault();
				this.$emit('update:query', '');
			},
			isDone(card: CardView): boolean {
				return !isPending(card);
			},
		},
	});
</script>
