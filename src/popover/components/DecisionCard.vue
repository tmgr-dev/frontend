<template>
	<div v-if="expanded" class="dec">
		<div class="dh">
			<span class="who">{{ sourceLabel }}</span>
			<span class="age">{{ age }}</span>
		</div>
		<div class="q">{{ question }}</div>
		<div v-if="answerable" class="opts">
			<button
				v-for="option in options"
				:key="option.id"
				type="button"
				data-nav
				:class="{ rec: option.recommended }"
				@click="$emit('answer', option.id)"
			>
				{{ option.label }}
			</button>
		</div>
	</div>
	<button
		v-else
		type="button"
		class="dec"
		data-nav
		@click="$emit('toggle', card.id)"
	>
		<span class="q">{{ question }}</span>
		<span class="chev">›</span>
	</button>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import type { CardView } from '@/decisions/types';
	import { ageText, isAnswerable, orderOptions } from '../model';

	export default defineComponent({
		emits: ['toggle', 'answer'],
		props: {
			card: { type: Object as PropType<CardView>, required: true },
			expanded: { type: Boolean, required: true },
		},
		computed: {
			question(): string {
				return this.card.question || this.card.title;
			},
			sourceLabel(): string {
				const { agent, title } = this.card.source;
				return title ? `${agent} · ${title}` : agent;
			},
			age(): string {
				return ageText(this.card.created_ms, Date.now());
			},
			answerable(): boolean {
				return isAnswerable(this.card);
			},
			options() {
				return orderOptions(this.card.options);
			},
		},
	});
</script>
