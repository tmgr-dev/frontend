<template>
	<div class="mh2" data-tauri-drag-region>
		<Avatar :agent="info.agent" :manager="isManager" />
		<div class="t" data-tauri-drag-region>
			<b data-tauri-drag-region
				>{{ info.title }}
				<span v-if="isManager" class="lockb"
					><Icon name="lock" />{{ $t('manager.header.encrypted') }}</span
				>
				<span
					v-if="info.unrestricted"
					class="unrb"
					:title="$t('manager.header.unrestrictedHint')"
					><Icon name="bolt" />{{ $t('manager.header.unrestricted') }}</span
				></b
			>
			<small
				v-if="modelText"
				class="mdlp"
				:title="info.model"
				data-tauri-drag-region
				>{{ modelText }}</small
			>
			<small v-if="sub" data-tauri-drag-region>{{ sub }}</small>
		</div>
		<button
			type="button"
			class="ibd"
			:aria-label="$t('manager.header.search')"
			:title="$t('manager.header.search')"
			@click="$emit('search')"
		>
			<Icon name="search" />
		</button>
		<button
			v-if="!isManager"
			type="button"
			class="btn sm"
			@click="$emit('switch')"
		>
			{{ $t('manager.header.switchAgent') }}
		</button>
		<template v-if="canControl">
			<template v-if="!exited">
				<button
					type="button"
					class="btn sm ctl"
					:disabled="busy"
					@click="$emit('stop')"
				>
					{{ $t('manager.header.stop') }}
				</button>
				<button
					type="button"
					class="btn sm ctl"
					:disabled="busy"
					@click="$emit('force')"
				>
					{{ $t('manager.header.forceStop') }}
				</button>
			</template>
			<button
				v-else
				type="button"
				class="btn sm ctl"
				:disabled="busy"
				@click="$emit('delete')"
			>
				{{ $t('manager.header.delete') }}
			</button>
		</template>
		<span v-if="info.state" class="hxx">
			<StatePill :state="info.state" />
			<span v-if="info.context_tokens !== undefined" :class="['ctx', ctxLevel]"
				><span class="bar"
					><i :style="{ width: `${info.context_pct ?? 0}%` }"></i></span
				><em>{{ ctxText }}</em></span
			>
		</span>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import type { RoomInfo } from '../types';
	import Avatar from './Avatar.vue';
	import Icon from './Icon.vue';
	import StatePill from './StatePill.vue';
	import { contextFull, contextLevel, modelPillText } from './viewHelpers';

	export default defineComponent({
		name: 'ManagerChatHeader',
		components: { Avatar, Icon, StatePill },
		props: {
			info: { type: Object as PropType<RoomInfo>, required: true },
			sub: { type: String, default: '' },
			busy: { type: Boolean, default: false },
		},
		emits: ['search', 'switch', 'stop', 'force', 'delete'],
		computed: {
			canControl(): boolean {
				return this.info.room.kind === 'session' && this.info.local !== false;
			},
			exited(): boolean {
				return this.info.state === 'exited';
			},
			isManager(): boolean {
				return this.info.room.kind === 'manager';
			},
			modelText(): string {
				return modelPillText(this.info);
			},
			ctxLevel(): string {
				return contextLevel(this.info.context_pct);
			},
			ctxText(): string {
				return contextFull(this.info.context_tokens, this.info.context_pct);
			},
		},
	});
</script>
