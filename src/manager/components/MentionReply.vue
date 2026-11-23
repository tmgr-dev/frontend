<template>
	<div class="mention" :data-state="mention.state" :data-role="role">
		<template v-if="role === 'question'">
			<p class="mstate" role="status">
				{{ $t(`manager.mention.state.${stateKey}`, { who }) }}
			</p>
		</template>
		<template v-else>
			<p class="mstate">{{ $t('manager.mention.replyFrom', { who }) }}</p>
			<div class="mact">
				<button
					type="button"
					class="btn small"
					@click="$emit('open', mention.target)"
				>
					{{ $t('manager.mention.open', { who }) }}
				</button>
				<button
					v-if="gate.show && !gate.refused"
					type="button"
					class="btn small"
					:disabled="busy || injected"
					@click="$emit('inject', message)"
				>
					{{
						injected
							? $t('manager.mention.added')
							: $t('manager.mention.addToContext')
					}}
				</button>
				<span v-else-if="gate.refused" class="mnote">{{
					$t('manager.mention.unrestricted')
				}}</span>
			</div>
		</template>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import { injectGate, mentionRole, type InjectGate } from '../mentions';
	import type { ChatMessage, MentionRef, RoomInfo } from '../types';

	const KNOWN = new Set([
		'queued',
		'sent',
		'answering',
		'answered',
		'busy',
		'offline',
		'timeout',
		'interrupted',
		'expired',
		'refused',
		'rate_limited',
		'cancelled',
	]);

	export default defineComponent({
		name: 'ManagerMentionReply',
		props: {
			message: { type: Object as PropType<ChatMessage>, required: true },
			latest: { type: Object as PropType<MentionRef>, required: true },
			origin: { type: Object as PropType<RoomInfo>, default: undefined },
			busy: { type: Boolean, default: false },
			injected: { type: Boolean, default: false },
		},
		emits: ['open', 'inject'],
		computed: {
			role(): 'question' | 'reply' {
				return mentionRole(this.message) === 'reply' ? 'reply' : 'question';
			},
			mention(): MentionRef {
				return this.role === 'reply'
					? (this.message.mention as MentionRef)
					: this.latest;
			},
			who(): string {
				return `@${this.mention.target_title}`;
			},
			stateKey(): string {
				return KNOWN.has(this.mention.state) ? this.mention.state : 'other';
			},
			gate(): InjectGate {
				return injectGate(this.origin, this.message);
			},
		},
	});
</script>
