<template>
	<div class="pane">
		<div ref="log" class="chat">
			<div v-if="messages.length === 0" class="placeholder">
				{{ $t('decisions.chat.empty') }}
			</div>
			<div
				v-for="message in messages"
				:key="message.seq"
				class="msg"
				:class="{ me: message.author === 'owner' }"
			>
				<div class="who">
					<template v-if="message.author === 'owner'">{{
						$t('decisions.chat.you')
					}}</template>
					<template v-else><i></i>{{ $t('decisions.chat.agent') }}</template>
				</div>
				<div
					v-if="isAgent(message)"
					class="md"
					@click="onMarkdownClick"
					v-html="render(message.text)"
				></div>
				<template v-else>{{ message.text }}</template>
			</div>
			<div v-if="notice" class="msg typing">{{ notice }}</div>
		</div>
		<div class="chatbox">
			<textarea
				ref="input"
				v-model="text"
				rows="1"
				data-input="chat"
				:disabled="!canReply"
				:placeholder="
					canReply
						? $t('decisions.chat.placeholder')
						: $t('decisions.chat.unavailable')
				"
				@keydown="onKey"
			></textarea>
			<div v-if="!canReply" class="hint">
				{{ $t('decisions.chat.answerInSession') }}
			</div>
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import { agentWaitNotice } from '../diff';
	import { autosizeTextarea } from '@/utils/commentComposer';
	import { isSubmitKey } from '../keys';
	import { clickedHttpLink, isAgentMessage, renderMarkdown } from '../markdown';
	import type { CardMessage } from '../types';

	export default defineComponent({
		name: 'ChatPane',
		props: {
			messages: { type: Array as PropType<CardMessage[]>, required: true },
			canReply: { type: Boolean, required: true },
			final: { type: Boolean, required: true },
			listening: { type: Boolean, required: true },
			queued: { type: Boolean, default: false },
			typed: { type: Boolean, default: false },
			sessionGone: { type: Boolean, required: true },
			now: { type: Number, required: true },
		},
		emits: ['send', 'open-link'],
		data() {
			return { text: '' };
		},
		computed: {
			notice(): string | null {
				return agentWaitNotice(this.messages, {
					listening: this.listening,
					queued: this.queued,
					typed: this.typed,
					sessionGone: this.sessionGone,
					canReply: this.canReply,
					final: this.final,
					now: this.now,
				});
			},
		},
		watch: {
			text() {
				void this.$nextTick(() =>
					autosizeTextarea(this.$refs.input as HTMLTextAreaElement | undefined),
				);
			},
			messages() {
				void this.$nextTick(() => {
					const log = this.$refs.log as HTMLElement | undefined;
					if (log) log.scrollTop = log.scrollHeight;
				});
			},
		},
		methods: {
			isAgent: isAgentMessage,
			render: renderMarkdown,
			onMarkdownClick(event: MouseEvent) {
				const href = clickedHttpLink(event);
				if (href) this.$emit('open-link', href);
			},
			focus() {
				(this.$refs.input as HTMLTextAreaElement | undefined)?.focus();
			},
			onKey(event: KeyboardEvent) {
				if (event.key === 'Escape') return;
				if (isSubmitKey(event) && this.text.trim()) {
					event.preventDefault();
					this.$emit('send', this.text.trim());
					this.text = '';
				}
			},
		},
	});
</script>
