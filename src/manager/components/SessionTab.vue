<template>
	<div class="peek">
		<template v-if="info && info.room.kind === 'session'">
			<div class="ph2">
				<Avatar :agent="info.agent" />
				<div>
					<b>{{ heading }}</b>
					<small>{{ subheading }}</small>
				</div>
				<StatePill v-if="info.state" :state="info.state" />
			</div>
			<div class="kv">
				<span>{{ $t('manager.panel.agent') }}</span
				><b>{{ info.agent ?? '—' }}</b>
				<span>{{ $t('manager.panel.machine') }}</span
				><b class="mono">{{ info.room.machine }}</b>
				<span>{{ $t('manager.panel.context') }}</span
				><b
					><ContextMeter
						v-if="info.context_tokens !== undefined"
						:tokens="info.context_tokens"
						:pct="info.context_pct"
						full
					/><template v-else>—</template></b
				>
				<template v-if="info.state">
					<span>{{ $t('manager.panel.state') }}</span
					><b>{{
						$te(`manager.state.${info.state}`)
							? $t(`manager.state.${info.state}`)
							: info.state
					}}</b>
				</template>
			</div>
			<div v-if="handoff" class="handoff">
				<b>{{ $t('manager.panel.handoff') }}</b>
				<small>{{ $t('manager.panel.handoffHint') }}</small>
			</div>
			<div class="lbl">{{ $t('manager.panel.lastMessages') }}</div>
			<ul v-if="previews.length" class="pv">
				<li v-for="p in previews" :key="p.id">
					<b>{{ p.who }}</b
					>{{ p.text }}
				</li>
			</ul>
			<p v-else class="hint">{{ $t('manager.list.noMessages') }}</p>
			<p class="hint">{{ $t('manager.panel.writeToAct') }}</p>
		</template>
		<p v-else class="hint">{{ $t('manager.panel.pickSession') }}</p>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import type { ChatMessage, RoomInfo } from '../types';
	import Avatar from './Avatar.vue';
	import ContextMeter from './ContextMeter.vue';
	import StatePill from './StatePill.vue';
	import { needsHandoff } from '../hierarchy';
	import { authorLabel, plainPreview } from './viewHelpers';

	export default defineComponent({
		name: 'ManagerSessionTab',
		components: { Avatar, ContextMeter, StatePill },
		props: {
			info: { type: Object as PropType<RoomInfo>, default: undefined },
			messages: { type: Array as PropType<ChatMessage[]>, default: () => [] },
		},
		computed: {
			heading(): string {
				const i = this.info as RoomInfo;
				return `${i.agent ?? this.$t('manager.panel.unknownAgent')} · ${i.room.machine}`;
			},
			subheading(): string {
				const { task, title } = this.info as RoomInfo;
				return task && task !== title ? `${task} · ${title}` : title;
			},
			handoff(): boolean {
				return needsHandoff(this.info?.context_tokens);
			},
			previews(): { id: string; who: string; text: string }[] {
				return this.messages
					.filter((m) => m.kind !== 'decision')
					.slice(-3)
					.map((m) => ({
						id: m.id,
						who: `${authorLabel(
							m.author,
							(k: string) => this.$t(k),
							m.user_turn,
						)}: `,
						text: plainPreview(m.body_md, 90),
					}));
			},
		},
	});
</script>
