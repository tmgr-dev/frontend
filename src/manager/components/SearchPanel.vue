<template>
	<div class="srch">
		<div class="sbox2">
			<Icon name="search" />
			<input
				type="text"
				:value="query"
				:placeholder="$t('manager.search.placeholder')"
				:aria-label="$t('manager.search.placeholder')"
				autocomplete="off"
				@input="
					$emit('update:query', ($event.target as HTMLInputElement).value)
				"
			/>
			<button
				v-if="query"
				type="button"
				:aria-label="$t('manager.search.clear')"
				@click="$emit('update:query', '')"
			>
				<Icon name="x" />
			</button>
		</div>
		<div class="sfil">
			<button
				type="button"
				:class="['sf2', { on: scope === 'all' }]"
				:aria-pressed="scope === 'all'"
				@click="$emit('update:scope', 'all')"
			>
				{{ $t('manager.search.scopeAll') }}
			</button>
			<button
				type="button"
				:class="['sf2', { on: scope === 'chat' }]"
				:aria-pressed="scope === 'chat'"
				@click="$emit('update:scope', 'chat')"
			>
				{{ $t('manager.search.scopeChat', { chat: chatLabel }) }}
			</button>
		</div>
		<div class="sres">
			<div v-if="!query.trim()" class="empty">
				<Icon name="search" /><b>{{ $t('manager.search.emptyTitle') }}</b
				><span>{{ $t('manager.search.emptyHint') }}</span>
			</div>
			<div v-else-if="hits.length === 0" class="empty">
				<Icon name="search" /><b>{{ $t('manager.search.noMatches') }}</b
				><span>{{ $t('manager.search.noMatchesHint') }}</span>
			</div>
			<template v-else>
				<div class="sc">
					{{
						$t('manager.search.results', { count: hits.length }, hits.length)
					}}
				</div>
				<button
					v-for="hit in hits"
					:key="hit.id"
					type="button"
					class="sr"
					@click="$emit('select', hit)"
				>
					<Avatar
						size="xs"
						:agent="hit.author.agent"
						:manager="
							hit.author.kind === 'manager' || hit.room.kind === 'manager'
						"
					/>
					<span class="srb">
						<span class="srh"
							><b>{{ titleOf(hit) }}</b
							><i>{{ formatClock(hit.ts_ms) }}</i></span
						>
						<span class="srt">
							<span v-if="hit.attachments?.length" class="srat"
								><Icon name="file"
							/></span>
							<em>{{ who(hit) }}:</em>{{ ' ' }}
							<template v-for="(part, i) in parts(hit)" :key="i"
								><mark v-if="part.hit">{{ part.text }}</mark
								><template v-else>{{ part.text }}</template></template
							>
						</span>
					</span>
				</button>
			</template>
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import { roomKey, type ChatMessage } from '../types';
	import Avatar from './Avatar.vue';
	import Icon from './Icon.vue';
	import {
		authorLabel,
		formatClock,
		highlightParts,
		snippetAround,
	} from './viewHelpers';

	export default defineComponent({
		name: 'ManagerSearchPanel',
		components: { Avatar, Icon },
		props: {
			query: { type: String, default: '' },
			scope: { type: String as PropType<'chat' | 'all'>, default: 'all' },
			chatLabel: { type: String, default: '' },
			hits: { type: Array as PropType<ChatMessage[]>, default: () => [] },
			roomTitles: {
				type: Object as PropType<Record<string, string>>,
				default: () => ({}),
			},
		},
		emits: ['update:query', 'update:scope', 'select'],
		methods: {
			formatClock,
			titleOf(hit: ChatMessage): string {
				return (
					this.roomTitles[roomKey(hit.room)] ??
					hit.room.session ??
					hit.room.machine
				);
			},
			who(hit: ChatMessage): string {
				return authorLabel(hit.author, (k) => this.$t(k), hit.user_turn);
			},
			parts(hit: ChatMessage) {
				return highlightParts(
					snippetAround(hit.body_md, this.query),
					this.query,
				);
			},
		},
	});
</script>
