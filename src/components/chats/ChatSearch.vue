<template>
	<div class="relative">
		<Input
			v-model="query"
			:placeholder="$t('chats.search.placeholder')"
			data-testid="chat-search"
			@input="schedule"
		/>
		<div
			v-if="query.trim()"
			class="absolute top-full right-0 left-0 z-10 mt-1 max-h-72 overflow-y-auto rounded-card border border-line bg-surface shadow-md"
		>
			<p v-if="searching" class="px-3 py-2 text-sm text-ink-subtle">
				{{ $t('common.states.loading') }}
			</p>
			<p v-else-if="!hits.length" class="px-3 py-2 text-sm text-ink-subtle">
				{{ $t('chats.search.empty') }}
			</p>
			<button
				v-for="hit in hits"
				:key="hit.message_id"
				type="button"
				class="block w-full px-3 py-2 text-left hover:bg-surface-hover"
				@click="open(hit)"
			>
				<p class="truncate text-xs text-ink-subtle">
					{{ hit.chat_title || hit.author.name }}
				</p>
				<p class="truncate text-sm text-ink">{{ hit.snippet }}</p>
			</button>
		</div>
	</div>
</template>

<script lang="ts">
	import { searchChats } from '@/actions/tmgr/chats';
	import type { ChatMessageHit } from '@/actions/tmgr/chats';
	import { Input } from '@/components/ui/input';
	import { defineComponent, onBeforeUnmount, ref } from 'vue';

	export default defineComponent({
		name: 'ChatSearch',
		components: { Input },
		props: {
			workspaceId: { type: Number, required: true },
		},
		emits: ['open'],
		setup(props, { emit }) {
			const query = ref('');
			const hits = ref<ChatMessageHit[]>([]);
			const searching = ref(false);
			let timer: ReturnType<typeof setTimeout> | null = null;

			const run = async () => {
				const q = query.value.trim();
				if (!q) return;
				try {
					const found = await searchChats(props.workspaceId, q);
					if (q === query.value.trim()) hits.value = found;
				} catch {
					hits.value = [];
				} finally {
					searching.value = false;
				}
			};

			const schedule = () => {
				if (timer) clearTimeout(timer);
				searching.value = true;
				timer = setTimeout(run, 300);
			};

			const open = (hit: ChatMessageHit) => {
				query.value = '';
				hits.value = [];
				emit('open', hit.chat_id);
			};

			onBeforeUnmount(() => {
				if (timer) clearTimeout(timer);
			});

			return { query, hits, searching, schedule, open };
		},
	});
</script>
