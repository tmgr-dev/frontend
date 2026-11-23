<template>
	<ul class="calls">
		<li
			v-for="(call, i) in calls"
			:key="call.n ?? i"
			:class="['call', { open: isOpen(call, i), err: call.error }]"
		>
			<button
				type="button"
				class="call-row"
				:aria-expanded="isOpen(call, i) ? 'true' : 'false'"
				:disabled="!expandable(call)"
				@click="toggle(call, i)"
			>
				<span class="call-name">{{ call.name }}</span>
				<code v-if="call.summary" class="call-sum">{{ call.summary }}</code>
				<span v-if="call.running" class="call-run" aria-hidden="true">…</span>
			</button>
			<pre v-if="isOpen(call, i) && call.result" class="call-out">{{
				call.result
			}}</pre>
		</li>
	</ul>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import type { ToolCall } from '../types';

	export default defineComponent({
		name: 'ManagerToolCallList',
		props: {
			calls: { type: Array as PropType<ToolCall[]>, required: true },
		},
		data() {
			return { opened: {} as Record<string, boolean> };
		},
		methods: {
			keyOf(call: ToolCall, i: number): string {
				return String(call.n ?? i);
			},
			expandable(call: ToolCall): boolean {
				return !!call.summary || !!call.result;
			},
			isOpen(call: ToolCall, i: number): boolean {
				return !!this.opened[this.keyOf(call, i)];
			},
			toggle(call: ToolCall, i: number) {
				if (!this.expandable(call)) return;
				const key = this.keyOf(call, i);
				this.opened = { ...this.opened, [key]: !this.opened[key] };
			},
		},
	});
</script>
