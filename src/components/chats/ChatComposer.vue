<template>
	<div class="relative">
		<p
			v-if="disabled"
			class="rounded-card border border-line bg-surface px-3 py-2 text-sm text-ink-subtle"
			data-testid="chat-composer-disabled"
		>
			{{ disabledHint }}
		</p>
		<div v-else>
			<ul
				v-if="suggestions.length"
				class="absolute bottom-full left-0 z-10 mb-1 max-h-48 w-64 overflow-y-auto rounded-card border border-line bg-surface shadow-md"
				data-testid="chat-mention-list"
			>
				<li v-for="(member, index) in suggestions" :key="member.id">
					<button
						type="button"
						class="w-full px-3 py-1.5 text-left text-sm text-ink hover:bg-surface-hover"
						:class="index === highlighted ? 'bg-surface-hover' : ''"
						@mousedown.prevent="pick(member)"
					>
						{{ member.name }}
					</button>
				</li>
			</ul>
			<ul v-if="files.length" class="mb-1 flex flex-wrap gap-1">
				<li
					v-for="(file, index) in files"
					:key="file.path"
					class="inline-flex items-center gap-1 rounded-md border border-line bg-surface px-2 py-1 text-xs text-ink"
				>
					{{ file.name }}
					<button
						type="button"
						:aria-label="$t('common.actions.remove')"
						@click="files.splice(index, 1)"
					>
						<X class="h-3 w-3" />
					</button>
				</li>
			</ul>
			<div
				class="flex items-end gap-2 rounded-card border border-line bg-surface p-2"
			>
				<button
					type="button"
					class="rounded p-1.5 text-ink-subtle hover:bg-surface-hover hover:text-ink"
					:title="$t('chats.attach')"
					:aria-label="$t('chats.attach')"
					@click="fileInput?.click()"
				>
					<Paperclip class="h-4 w-4" />
				</button>
				<input
					ref="fileInput"
					type="file"
					multiple
					class="hidden"
					@change="onFiles"
				/>
				<textarea
					ref="textarea"
					v-model="text"
					rows="1"
					class="max-h-40 min-h-[2rem] flex-1 resize-none bg-transparent text-sm text-ink outline-hidden placeholder:text-ink-subtle"
					:placeholder="placeholder"
					data-testid="chat-composer-input"
					@input="onInput"
					@keydown="onKeydown"
					@click="updateMention"
				></textarea>
				<Button
					size="icon"
					:disabled="busy || (!text.trim() && !files.length)"
					:aria-label="$t('chats.send')"
					data-testid="chat-send"
					@click="submit"
				>
					<Send />
				</Button>
			</div>
			<p v-if="failed" class="mt-1 text-xs text-destructive">
				{{ $t('chats.sendFailed') }}
			</p>
		</div>
	</div>
</template>

<script lang="ts">
	import type {
		ChatFileUpload,
		ChatMention,
		SendMessagePayload,
	} from '@/actions/tmgr/chats';
	import { uploadChatFile } from '@/actions/tmgr/chats';
	import { Button } from '@/components/ui/button';
	import { autosizeTextarea, isEnterSend } from '@/utils/commentComposer';
	import {
		findMentionQuery,
		insertMention,
		type MentionQuery,
	} from '@/utils/chatMentions';
	import { Paperclip, Send, X } from 'lucide-vue-next';
	import { computed, defineComponent, nextTick, ref, type PropType } from 'vue';

	interface Member {
		id: number;
		name: string;
	}

	export default defineComponent({
		name: 'ChatComposer',
		components: { Button, Paperclip, Send, X },
		props: {
			members: { type: Array as PropType<Member[]>, default: () => [] },
			disabled: { type: Boolean, default: false },
			disabledHint: { type: String, default: '' },
			placeholder: { type: String, default: '' },
			submitMessage: {
				type: Function as PropType<
					(
						payload: Pick<SendMessagePayload, 'body' | 'mentions' | 'files'>,
					) => Promise<void>
				>,
				required: true,
			},
		},
		setup(props) {
			const text = ref('');
			const files = ref<ChatFileUpload[]>([]);
			const mentions = ref<Member[]>([]);
			const mention = ref<MentionQuery | null>(null);
			const highlighted = ref(0);
			const busy = ref(false);
			const failed = ref(false);
			const textarea = ref<HTMLTextAreaElement | null>(null);
			const fileInput = ref<HTMLInputElement | null>(null);

			const suggestions = computed(() => {
				if (!mention.value) return [];
				const q = mention.value.query.toLowerCase();
				return props.members
					.filter((m) => m.name.toLowerCase().includes(q))
					.slice(0, 8);
			});

			const updateMention = () => {
				const el = textarea.value;
				mention.value = el
					? findMentionQuery(text.value, el.selectionStart ?? text.value.length)
					: null;
				highlighted.value = 0;
			};

			const onInput = () => {
				autosizeTextarea(textarea.value);
				updateMention();
			};

			const pick = async (member: Member) => {
				const el = textarea.value;
				if (!el || !mention.value) return;
				const caret = el.selectionStart ?? text.value.length;
				const next = insertMention(
					text.value,
					caret,
					mention.value,
					member.name,
				);
				text.value = next.text;
				if (!mentions.value.some((m) => m.id === member.id)) {
					mentions.value.push(member);
				}
				mention.value = null;
				await nextTick();
				el.setSelectionRange(next.caret, next.caret);
				el.focus();
				autosizeTextarea(el);
			};

			const onFiles = async (event: Event) => {
				const input = event.target as HTMLInputElement;
				const picked = Array.from(input.files ?? []);
				input.value = '';
				busy.value = true;
				try {
					for (const file of picked)
						files.value.push(await uploadChatFile(file));
				} catch {
					failed.value = true;
				} finally {
					busy.value = false;
				}
			};

			const submit = async () => {
				const body = text.value.trim();
				if (busy.value || (!body && !files.value.length)) return;
				busy.value = true;
				failed.value = false;
				const payload = {
					body,
					mentions: mentions.value
						.filter((m) => body.includes(`@${m.name}`))
						.map((m): ChatMention => ({ kind: 'user', id: m.id })),
					files: files.value,
				};
				try {
					await props.submitMessage(payload);
					text.value = '';
					files.value = [];
					mentions.value = [];
					await nextTick();
					autosizeTextarea(textarea.value);
				} catch {
					failed.value = true;
				} finally {
					busy.value = false;
				}
			};

			const onKeydown = (event: KeyboardEvent) => {
				if (suggestions.value.length) {
					if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
						event.preventDefault();
						const step = event.key === 'ArrowDown' ? 1 : -1;
						const n = suggestions.value.length;
						highlighted.value = (highlighted.value + step + n) % n;
						return;
					}
					if (event.key === 'Enter' || event.key === 'Tab') {
						event.preventDefault();
						void pick(suggestions.value[highlighted.value]);
						return;
					}
					if (event.key === 'Escape') {
						mention.value = null;
						return;
					}
				}
				if (isEnterSend(event)) {
					event.preventDefault();
					void submit();
				}
			};

			return {
				text,
				files,
				suggestions,
				highlighted,
				busy,
				failed,
				textarea,
				fileInput,
				onInput,
				onKeydown,
				onFiles,
				updateMention,
				pick,
				submit,
			};
		},
	});
</script>
