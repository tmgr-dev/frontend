<template>
	<div v-if="isSystem" class="sys">
		<span>{{ message.body_md }}</span>
	</div>
	<div v-else class="mw" :data-mid="message.id">
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
			<div class="meta">{{ clock }}</div>
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
			</div>
		</div>
		<div class="macts">
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
	import type { Attachment, ChatMessage } from '../types';
	import Avatar from './Avatar.vue';
	import Icon from './Icon.vue';
	import {
		addCopyButtons,
		copyBlockText,
		copyText,
		flashCopied,
	} from '../format';
	import {
		authorLabel,
		formatClock,
		formatSize,
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
		components: { Attachments, Avatar, Icon },
		props: {
			message: { type: Object as PropType<ChatMessage>, required: true },
			html: { type: String, default: undefined },
			replyTo: { type: Object as PropType<ChatMessage>, default: undefined },
			grouped: { type: Boolean, default: false },
		},
		emits: ['reply', 'copy'],
		computed: {
			isSystem(): boolean {
				return (
					this.message.author.kind === 'system' ||
					(this.message.kind !== 'text' && this.message.kind !== 'decision')
				);
			},
			isOwner(): boolean {
				return this.message.author.kind === 'owner' || !!this.message.user_turn;
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
