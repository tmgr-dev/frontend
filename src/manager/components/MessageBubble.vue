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
				<time :datetime="isoTime" :title="fullTime">{{ clock }}</time>
				<template v-if="queued">
					<span :title="$t('manager.messages.queuedHint')">{{
						queuedText
					}}</span>
					<button
						type="button"
						class="qcancel"
						@click="$emit('cancel-queued', message.id)"
					>
						{{ $t('manager.messages.cancelQueued') }}
					</button>
				</template>
				<span
					v-if="deliveryText"
					:class="['dlv', delivery && `dlv-${delivery.state}`]"
					role="status"
					>{{ deliveryText }}</span
				>
				<button
					v-if="showRetry"
					type="button"
					class="qcancel"
					:title="
						delivery?.state === 'unconfirmed'
							? $t('manager.messages.delivery.resendHint')
							: undefined
					"
					@click="$emit('retry', message.id)"
				>
					{{
						delivery?.state === 'unconfirmed'
							? $t('manager.messages.delivery.resend')
							: $t('manager.messages.delivery.retry')
					}}
				</button>
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
				<div v-if="!grouped" class="who">{{ labelOf(message) }}</div>
				<div :class="isDecision ? 'dwrap' : 'bubble abub'">
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
				<div class="meta">
					<time :datetime="isoTime" :title="fullTime">{{ clock }}</time>
				</div>
				<details v-if="tools?.length" class="tools">
					<summary>
						{{ $t('manager.activity.toolsUsed', { count: tools.length }) }}
					</summary>
					<ToolCallList v-if="calls?.length" :calls="calls" />
					<ul v-else>
						<li v-for="(tool, i) in tools" :key="i">{{ tool }}</li>
					</ul>
				</details>
				<div v-if="showVoice && !pack.installed" class="voice">
					<template v-if="pack.installing">
						<span>{{
							$t('manager.messages.voiceDownloading', {
								percent: voicePercent,
							})
						}}</span>
						<button type="button" @click="pack.cancel()">
							{{ $t('manager.messages.cancelVoiceDownload') }}
						</button>
					</template>
					<template v-else>
						<span v-if="pack.error">{{
							$t('manager.messages.voiceDownloadFailed', { language })
						}}</span>
						<button
							type="button"
							:title="$t('manager.messages.voiceOnDevice')"
							@click="pack.install()"
						>
							{{
								$t('manager.messages.downloadVoice', {
									language,
									size: voiceSizeMb,
								})
							}}
						</button>
					</template>
				</div>
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
				@click="$emit('copy', message.body_md)"
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
		canRetry,
		isQueuedWhy,
		REVOKE_PENDING_REASON,
		SEND_TIMEOUT_REASON,
		type Delivery,
	} from '../delivery';
	import {
		addCopyButtons,
		copyBlockText,
		copyText,
		flashCopied,
	} from '../format';
	import { hasPollQuestions } from '../pollCards';
	import { externalVoice } from '../tts/externalVoice';
	import { prepareSpeech } from '../tts/prepareText';
	import { speaker } from '../tts/speaker';
	import { voicePacks } from '../tts/voicePack';
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
			queuedWhy: { type: String, default: undefined },
			tools: { type: Array as PropType<string[]>, default: undefined },
			calls: { type: Array as PropType<ToolCall[]>, default: undefined },
			delivery: { type: Object as PropType<Delivery>, default: undefined },
			deliveryWho: { type: String, default: '' },
			deliveryMachine: { type: String, default: '' },
		},
		emits: ['reply', 'copy', 'cancel-queued', 'retry'],
		data() {
			return { externalVoice };
		},
		mounted() {
			if (this.voiceEligible && !this.pack.statusRequested) {
				this.pack.statusRequested = true;
				void this.pack.refresh();
			}
		},
		computed: {
			showRetry(): boolean {
				return canRetry(this.delivery) && !this.message.mention;
			},
			queuedText(): string {
				return isQueuedWhy(this.queuedWhy)
					? this.$t(`manager.messages.queuedWhy.${this.queuedWhy}`)
					: this.$t('manager.messages.queued');
			},
			deliveryText(): string {
				const d = this.delivery;
				if (!d) return '';
				if (d.retried) return this.$t('manager.messages.delivery.retried');
				if (d.state === 'failed' && d.reason === SEND_TIMEOUT_REASON)
					return this.$t('manager.messages.delivery.sendTimeout');
				if (d.state === 'failed' && d.reason === REVOKE_PENDING_REASON)
					return this.$t('manager.messages.delivery.revokePending', {
						machine: this.deliveryMachine,
					});
				if (d.state === 'failed' && d.reason)
					return this.$t('manager.messages.delivery.failedReason', {
						reason: d.reason,
					});
				if (d.state === 'queued' && isQueuedWhy(d.reason))
					return this.$t(`manager.messages.queuedWhy.${d.reason}`);
				return this.$t(`manager.messages.delivery.${d.state}`, {
					who: this.deliveryWho,
				});
			},
			voiceEligible(): boolean {
				return (
					!this.isSystem &&
					!this.isOwner &&
					!this.isDecision &&
					this.message.body_md.trim().length > 0
				);
			},
			voiceLang(): 'en' | 'ru' {
				return prepareSpeech(this.message.body_md).lang;
			},
			pack() {
				return voicePacks[this.voiceLang];
			},
			language(): string {
				return this.$t(`manager.messages.languageName.${this.voiceLang}`);
			},
			showVoice(): boolean {
				return this.voiceEligible && this.pack.supported;
			},
			voiceSizeMb(): number {
				return Math.round(this.pack.sizeBytes / 1024 / 1024);
			},
			voicePercent(): number {
				return Math.round(this.pack.progress * 100);
			},
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
				return (
					this.message.kind === 'decision' || hasPollQuestions(this.message)
				);
			},
			isManagerAuthor(): boolean {
				return this.message.author.kind === 'manager';
			},
			clock(): string {
				return formatClock(this.message.ts_ms);
			},
			isoTime(): string {
				const d = new Date(this.message.ts_ms);
				return Number.isNaN(d.getTime()) ? '' : d.toISOString();
			},
			fullTime(): string {
				return new Date(this.message.ts_ms).toLocaleString(undefined, {
					weekday: 'short',
					day: 'numeric',
					month: 'short',
					year: 'numeric',
					hour: '2-digit',
					minute: '2-digit',
					second: '2-digit',
				});
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
				if (speaker.error === 'no-voice')
					return this.$t('manager.messages.noVoice');
				if (speaker.error?.startsWith('external-')) {
					const reason = this.externalVoice.health?.lastError;
					const base = this.$t('manager.messages.externalVoiceFailed');
					return reason ? `${base}: ${reason}` : base;
				}
				return this.$t('manager.messages.speakFailed');
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
