<template>
	<div class="scroll" role="log" :aria-label="$t('manager.messages.label')">
		<div class="inner">
			<div v-if="messages.length === 0" class="empty">
				<Icon name="term" /><b>{{ $t('manager.messages.empty') }}</b
				><span>{{ $t('manager.messages.emptyHint') }}</span>
			</div>
			<template v-for="row in rows" :key="row.message.id">
				<div v-if="row.day" class="day">{{ row.day }}</div>
				<MessageBubble
					:message="row.message"
					:html="bodyHtml[row.message.id]"
					:reply-to="row.replyTo"
					:grouped="row.grouped"
					@reply="$emit('reply', $event)"
					@copy="$emit('copy', $event)"
				>
					<template #decision="slotProps">
						<slot name="decision" :message="slotProps.message"></slot>
					</template>
				</MessageBubble>
			</template>
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import type { ChatMessage } from '../types';
	import Icon from './Icon.vue';
	import MessageBubble from './MessageBubble.vue';
	import { dayKey, formatDay, isGrouped } from './viewHelpers';

	interface Row {
		message: ChatMessage;
		day?: string;
		replyTo?: ChatMessage;
		grouped: boolean;
	}

	export default defineComponent({
		name: 'ManagerMessageList',
		components: { Icon, MessageBubble },
		props: {
			messages: { type: Array as PropType<ChatMessage[]>, required: true },
			bodyHtml: {
				type: Object as PropType<Record<string, string>>,
				default: () => ({}),
			},
			now: { type: Number, default: () => Date.now() },
		},
		emits: ['reply', 'copy'],
		computed: {
			rows(): Row[] {
				const byId = new Map(this.messages.map((m) => [m.id, m]));
				const labels = {
					today: this.$t('manager.messages.today'),
					yesterday: this.$t('manager.messages.yesterday'),
				};
				return this.messages.map((message, i) => {
					const prev = this.messages[i - 1];
					const newDay = !prev || dayKey(prev.ts_ms) !== dayKey(message.ts_ms);
					return {
						message,
						day: newDay
							? formatDay(message.ts_ms, this.now, labels)
							: undefined,
						replyTo: message.reply_to ? byId.get(message.reply_to) : undefined,
						grouped: isGrouped(prev, message),
					};
				});
			},
		},
	});
</script>
