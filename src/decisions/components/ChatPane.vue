<template>
	<div class="pane">
		<div ref="log" class="chat">
			<div v-if="messages.length === 0" class="placeholder">
				No messages on this card yet.
			</div>
			<div
				v-for="message in messages"
				:key="message.seq"
				class="msg"
				:class="{ me: message.author === 'owner' }"
			>
				<div class="who">
					<template v-if="message.author === 'owner'">You</template>
					<template v-else><i></i>Agent</template>
				</div>
				{{ message.text }}
			</div>
		</div>
		<div class="chatbox">
			<input
				ref="input"
				v-model="text"
				data-input="chat"
				:disabled="!canReply"
				:placeholder="
					canReply
						? 'Ask the agent about this card… Enter'
						: 'Chat is unavailable'
				"
				@keydown="onKey"
			/>
			<div v-if="!canReply" class="hint">Answer in the agent session.</div>
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import { isSubmitKey } from '../keys';
	import type { CardMessage } from '../types';

	export default defineComponent({
		name: 'ChatPane',
		props: {
			messages: { type: Array as PropType<CardMessage[]>, required: true },
			canReply: { type: Boolean, required: true },
		},
		emits: ['send'],
		data() {
			return { text: '' };
		},
		watch: {
			messages() {
				void this.$nextTick(() => {
					const log = this.$refs.log as HTMLElement | undefined;
					if (log) log.scrollTop = log.scrollHeight;
				});
			},
		},
		methods: {
			focus() {
				(this.$refs.input as HTMLInputElement | undefined)?.focus();
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
