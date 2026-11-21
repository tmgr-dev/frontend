<template>
	<div class="dcard" :data-card="cardId">
		<p v-if="!state.card && !state.error" class="dc-note">
			{{ $t('manager.decision.loading') }}
		</p>
		<template v-if="state.card">
			<div class="dc-title">{{ state.card.title }}</div>
			<div v-if="state.card.question" class="dc-q">
				{{ state.card.question }}
			</div>
			<div v-if="mode === 'final'" class="dc-note dc-final">
				{{ finalText }}
			</div>
			<p v-else-if="mode === 'action'" class="dc-note">
				{{ $t('manager.decision.openInDecisions') }}
			</p>
			<template v-else>
				<div class="dc-opts">
					<button
						v-for="option in state.card.options"
						:key="option.id"
						type="button"
						:class="['dc-opt', { rec: option.recommended }]"
						:disabled="state.busy"
						@click="pick(option.id)"
					>
						{{ option.label }}
					</button>
				</div>
				<form class="dc-free" @submit.prevent="sendFree">
					<input
						v-model="free"
						type="text"
						:placeholder="$t('manager.decision.freePlaceholder')"
						:disabled="state.busy"
					/>
					<button type="submit" :disabled="state.busy || !free.trim()">
						{{ $t('manager.decision.send') }}
					</button>
				</form>
			</template>
		</template>
		<p v-if="state.error" class="dc-note dc-err" role="alert">
			{{ state.error }}
		</p>
	</div>
</template>

<script lang="ts">
	import { t } from '@/i18n';
	import { defineComponent } from 'vue';
	import { answerSummary } from '../../decisions/answer';
	import {
		decisionsAnswer,
		decisionsGet,
		onDecisionsChanged,
	} from '../../decisions/api';
	import {
		answerState,
		choiceAnswer,
		createDecisionCardLoader,
		freeTextAnswer,
	} from '../decisionCard';

	export default defineComponent({
		name: 'ManagerDecisionCard',
		props: { cardId: { type: String, required: true } },
		setup(props) {
			return createDecisionCardLoader(
				{ get: decisionsGet, answer: decisionsAnswer },
				props.cardId,
			);
		},
		data() {
			return { free: '', unlisten: null as null | (() => void), gone: false };
		},
		computed: {
			mode() {
				return this.state.card ? answerState(this.state.card) : 'answer';
			},
			finalText(): string {
				const card = this.state.card;
				if (!card) return '';
				return card.answer
					? answerSummary(card, card.answer)
					: t('decisions.card.closed');
			},
		},
		mounted() {
			void this.load();
			void onDecisionsChanged(() => void this.load())
				.then((off) => {
					if (this.gone) off();
					else this.unlisten = off;
				})
				.catch(() => undefined);
		},
		beforeUnmount() {
			this.gone = true;
			this.unlisten?.();
		},
		methods: {
			pick(id: string) {
				if (this.state.card)
					void this.submit(choiceAnswer(this.state.card, id));
			},
			sendFree() {
				const answer = this.state.card
					? freeTextAnswer(this.state.card, this.free)
					: null;
				if (!answer) return;
				this.free = '';
				void this.submit(answer);
			},
		},
	});
</script>
