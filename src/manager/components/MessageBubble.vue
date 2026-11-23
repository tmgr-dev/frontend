<template>
	<div v-if="isSystem" class="sys">
		<span>{{ message.body_md }}</span>
	</div>
	<div v-else :class="['mw', { flash: message.fresh }]" :data-mid="message.id">
		<div v-if="isOwner" class="msg owner">
			<div class="bubble">
				<div v-if="replyTo" class="qt">
					<b>{{ labelOf(replyTo) }}</b
					><span>{{ truncate(replyTo.body_md, 90) }}</span>
				</div>
				<Attachments :items="message.attachments" />
				<div v-if="html" class="txt" v-html="html"></div>
				<div v-else class="txt plain">{{ message.body_md }}</div>
			</div>
			<div class="meta">
				{{ clock }}
				<template v-if="queued">
					<span :title="$t('manager.messages.queuedHint')">{{
						$t('manager.messages.queued')
					}}</span>
					<button
						type="button"
						class="qcancel"
						@click="$emit('cancel-queued', message.id)"
					>
						{{ $t('manager.messages.cancelQueued') }}
					</button>
				</template>
			</div>
		</div>
		<div v-else :class="['msg', 'mgr', { grouped }]">
			<Avatar
				v-if="!grouped"
				:agent="message.author.agent"
				:manager="isManagerAuthor"
			/>
			<span v-else class="av-spacer"></span>
			<div class="body">
				<div v-if="!grouped" class="who">
					{{ labelOf(message) }} <i>{{ clock }}</i>
				</div>
				<div v-if="replyTo" class="qt">
					<b>{{ labelOf(replyTo) }}</b
					><span>{{ truncate(replyTo.body_md, 90) }}</span>
				</div>
				<slot v-if="isDecision" name="decision" :message="message"></slot>
				<template v-else>
					<Attachments :items="message.attachments" />
					<div
						v-if="html"
						class="txt"
						v-html="copyableHtml"
						@click="onBodyClick"
					></div>
					<div v-else class="txt plain">{{ message.body_md }}</div>
				</template>
				<details v-if="tools?.length" class="tools">
					<summary>
						{{ $t('manager.activity.toolsUsed', { count: tools.length }) }}
					</summary>
					<ToolCallList v-if="calls?.length" :calls="calls" />
					<ul v-else>
						<li v-for="(tool, i) in tools" :key="i">{{ tool }}</li>
					</ul>
				</details>
			</div>
		</div>
		<div class="macts">
			<button
				v-if="canSpeak"
				type="button"
				:aria-pressed="speaking ? 'true' : 'false'"
				:aria-label="speakLabel"
				:title="speakTitle"
				@click="onSpeak"
			>
				<Icon :name="speaking ? 'stop' : 'speak'" />
			</button>
			<button
				type="button"
				:aria-label="$t('manager.messages.copy')"
				:title="$t('manager.messages.copy')"
				@click="$emit('copy', message)"
			>
				<Icon name="copy" />
			</button>
			<button
				type="button"
				:aria-label="$t('manager.messages.reply')"
				:title="$t('manager.messages.reply')"
				@click="$emit('reply', message)"
			>
				<Icon name="reply" />
			</button>
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent, h, type PropType } from 'vue';
	import {
		addCopyButtons,
		copyBlockText,
		copyText,
		flashCopied,
	} from '../format';
	import { speaker } from '../tts/speaker';
	import type { Attachment, ChatMessage, ToolCall } from '../types';
	import Avatar from './Avatar.vue';
	import Icon from './Icon.vue';
	import ToolCallList from './ToolCallList.vue';
	import {
		authorLabel,
		formatClock,
		formatSize,
		isOwnerMessage,
		truncate,
	} from './viewHelpers';

	const Attachments = defineComponent({
		name: 'ManagerAttachments',
		props: {
			items: { type: Array as PropType<Attachment[]>, default: undefined },
		},
		render() {
			if (!this.items?.length) return null;
			return h(
				'div',
				{ class: 'atts' },
				this.items.map((a) =>
					h('div', { class: 'file', key: a.sha256 || a.name }, [
						h('span', { class: 'fi' }, [h(Icon, { name: 'file' })]),
						h('div', [h('b', a.name), h('small', formatSize(a.size))]),
					]),
				),
			);
		},
	});

	export default defineComponent({
		name: 'ManagerMessageBubble',
		components: { Attachments, Avatar, Icon, ToolCallList },
		props: {
			message: { type: Object as PropType<ChatMessage>, required: true },
			html: { type: String, default: undefined },
			replyTo: { type: Object as PropType<ChatMessage>, default: undefined },
			grouped: { type: Boolean, default: false },
			queued: { type: Boolean, default: false },
			tools: { type: Array as PropType<string[]>, default: undefined },
			calls: { type: Array as PropType<ToolCall[]>, default: undefined },
		},
		emits: ['reply', 'copy', 'cancel-queued'],
		computed: {
			isSystem(): boolean {
				return (
					this.message.author.kind === 'system' ||
					(this.message.kind !== 'text' && this.message.kind !== 'decision')
				);
			},
			isOwner(): boolean {
				return isOwnerMessage(this.message);
			},
			isDecision(): boolean {
				return this.message.kind === 'decision';
			},
			isManagerAuthor(): boolean {
				return this.message.author.kind === 'manager';
			},
			clock(): string {
				return formatClock(this.message.ts_ms);
			},
			canSpeak(): boolean {
				return (
					!this.isOwner &&
					!this.isDecision &&
					this.message.body_md.trim().length > 0 &&
					speaker.available
				);
			},
			speaking(): boolean {
				return speaker.speakingId === this.message.id;
			},
			speakLabel(): string {
				return this.speaking
					? this.$t('manager.messages.stopSpeaking')
					: this.$t('manager.messages.speak');
			},
			speakTitle(): string {
				if (speaker.errorId !== this.message.id) return this.speakLabel;
				return speaker.error === 'no-voice'
					? this.$t('manager.messages.noVoice')
					: this.$t('manager.messages.speakFailed');
			},
			copyableHtml(): string | undefined {
				return this.html
					? addCopyButtons(this.html, {
							copy: this.$t('manager.messages.copy'),
							copied: this.$t('manager.messages.copied'),
						})
					: undefined;
			},
		},
		methods: {
			truncate,
			onSpeak() {
				speaker.toggle(this.message.id, this.message.body_md);
			},
			onBodyClick(e: MouseEvent) {
				const btn = (e.target as Element).closest<HTMLElement>('.blk-copy');
				if (!btn) return;
				void copyText(copyBlockText(btn.parentElement)).then((ok) => {
					if (ok) flashCopied(btn);
				});
			},
			labelOf(m: ChatMessage): string {
				return authorLabel(m.author, (k) => this.$t(k), m.user_turn);
			},
		},
	});
</script>
