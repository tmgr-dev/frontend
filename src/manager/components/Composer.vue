<template>
	<div class="comp">
		<div class="cin">
			<slot name="popover"></slot>
			<div class="cbox" @dragover.prevent @drop.prevent="onDrop">
				<div v-if="replyTo" class="rbar">
					<span class="rbi"><Icon name="reply" /></span>
					<div class="rq">
						<b>{{ $t('manager.composer.replyingTo', { who: replyWho }) }}</b
						><span>{{ truncate(replyTo.body_md, 110) }}</span>
					</div>
					<button
						type="button"
						:aria-label="$t('manager.composer.cancelReply')"
						@click="$emit('clear-reply')"
					>
						<Icon name="x" />
					</button>
				</div>
				<div v-if="attachments.length > 0" class="pend">
					<span
						v-for="(a, i) in attachments"
						:key="a.sha256 || a.name"
						class="pa"
					>
						<span class="fi"><Icon name="file" /></span>
						<em>{{ a.name }}</em>
						<button
							type="button"
							:aria-label="
								$t('manager.composer.removeAttachment', { name: a.name })
							"
							@click="$emit('remove-attachment', i)"
						>
							<Icon name="x" />
						</button>
					</span>
				</div>
				<div class="in">
					<button
						type="button"
						class="clip"
						:aria-label="$t('manager.composer.attach')"
						:title="$t('manager.composer.attach')"
						@click="$emit('attach')"
					>
						<Icon name="clip" />
					</button>
					<textarea
						ref="input"
						rows="1"
						:value="modelValue"
						:placeholder="placeholder || $t('manager.composer.placeholder')"
						:aria-label="placeholder || $t('manager.composer.placeholder')"
						@input="onInput"
						@keydown="onKeydown"
					></textarea>
					<button
						type="button"
						class="go"
						:aria-label="$t('manager.composer.send')"
						:title="$t('manager.composer.send')"
						:disabled="!canSend"
						@click="$emit('send')"
					>
						<Icon name="send" />
					</button>
				</div>
			</div>
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import type { Attachment, ChatMessage } from '../types';
	import Icon from './Icon.vue';
	import { authorLabel, truncate } from './viewHelpers';

	export default defineComponent({
		name: 'ManagerComposer',
		components: { Icon },
		props: {
			modelValue: { type: String, default: '' },
			placeholder: { type: String, default: '' },
			replyTo: { type: Object as PropType<ChatMessage>, default: undefined },
			attachments: { type: Array as PropType<Attachment[]>, default: () => [] },
			disabled: { type: Boolean, default: false },
		},
		emits: [
			'update:modelValue',
			'send',
			'clear-reply',
			'remove-attachment',
			'attach',
			'drop',
			'keydown',
		],
		computed: {
			canSend(): boolean {
				return (
					!this.disabled &&
					(this.modelValue.trim() !== '' || this.attachments.length > 0)
				);
			},
			replyWho(): string {
				return this.replyTo
					? authorLabel(this.replyTo.author, (k) => this.$t(k))
					: '';
			},
		},
		methods: {
			truncate,
			focus(): void {
				(this.$refs.input as HTMLTextAreaElement | undefined)?.focus();
			},
			onInput(e: Event): void {
				const el = e.target as HTMLTextAreaElement;
				el.style.height = 'auto';
				el.style.height = `${Math.min(el.scrollHeight, 160)}px`;
				this.$emit('update:modelValue', el.value);
			},
			onKeydown(e: KeyboardEvent): void {
				this.$emit('keydown', e);
				if (e.defaultPrevented) return;
				if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
					e.preventDefault();
					if (this.canSend) this.$emit('send');
				}
			},
			onDrop(e: DragEvent): void {
				const files = Array.from(e.dataTransfer?.files ?? []);
				if (files.length > 0) this.$emit('drop', files);
			},
		},
	});
</script>
