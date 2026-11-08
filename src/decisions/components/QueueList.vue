<template>
	<nav class="sidebar">
		<div class="ws">
			<div class="logo">⚡</div>
			<div class="ws-name">
				<b>Decisions</b>
				<small class="doom-sub">{{ $t(subtitle) }}</small>
			</div>
			<span v-if="progress.pending" class="badge">{{ progress.pending }}</span>
		</div>
		<div class="sect">
			{{ $t('decisions.queue.title')
			}}<span>{{
				$t('decisions.queue.count', {
					pending: progress.pending,
					total: progress.total,
				})
			}}</span>
		</div>
		<div class="prog"><i :style="{ width: progress.percent + '%' }"></i></div>
		<input
			type="text"
			autocomplete="off"
			class="queue-search"
			:placeholder="$t('decisions.queue.search')"
			:aria-label="$t('decisions.queue.search')"
			:value="query"
			@input="onInput"
			@keydown.esc="clearQuery"
		/>
		<div
			v-if="progress.total > 0 && progress.pending === 0"
			class="doom-complete"
		>
			<span class="grass" aria-hidden="true">🌱</span>
			{{ $t('decisions.queue.complete') }}
		</div>
		<div v-if="groups.length === 0" class="empty-queue">
			{{
				query.trim() && progress.total > 0
					? $t('decisions.queue.noMatches')
					: $t('decisions.queue.empty')
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
							kindLabel(card.kind)
						}}</span>
						<span v-if="card.state === 'deferred'" class="k">{{
							$t('decisions.queue.deferred')
						}}</span>
						<span v-if="card.updated_ms" class="upd">{{
							$t('decisions.queue.updated')
						}}</span>
						<span v-if="card.unpicked" class="unpicked">{{
							$t('decisions.unpicked')
						}}</span>
						<span
							v-if="card.blocking && !isDone(card) && !card.session_gone"
							class="block"
							:title="$t('decisions.queue.agentWaiting')"
						></span>
					</div>
					<span class="t">{{ card.title }}</span>
				</button>
				<button
					v-if="!isDone(card)"
					type="button"
					class="close"
					:aria-label="$t('decisions.queue.closeCard')"
					:title="$t('decisions.queue.closeCard')"
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
	import { kindLabel } from '../format';
	import { isPending, type QueueGroup, type QueueProgress } from '../queue';
	import type { CardView } from '../types';

	export default defineComponent({
		name: 'QueueList',
		props: {
			groups: { type: Array as PropType<QueueGroup[]>, required: true },
			selectedId: { type: String as PropType<string | null>, default: null },
			progress: { type: Object as PropType<QueueProgress>, required: true },
			query: { type: String, default: '' },
			subtitle: { type: String, default: 'decisions.subtitle.default' },
		},
		emits: ['select', 'dismiss', 'update:query'],
		methods: {
			kindLabel,
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
