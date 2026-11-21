<template>
	<div class="run">
		<button
			type="button"
			class="open-btn"
			data-nav
			@click="$emit('action', openId)"
		>
			<span class="code"><span class="dot"></span>{{ code }}</span>
			<span class="title">{{ title }}</span>
		</button>
		<div class="time">
			{{ clock }}
			<button
				type="button"
				class="stop"
				data-nav
				:aria-label="$t('popover.running.stop')"
				@click="$emit('action', stopId)"
			>
				<i></i>
			</button>
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import {
		actionIds,
		formatClock,
		runningElapsed,
		stripTicket,
		ticketCode,
	} from '../model';
	import type { PopoverTask } from '../types';

	export default defineComponent({
		emits: ['action'],
		props: {
			task: { type: Object as PropType<PopoverTask>, required: true },
			clockOffset: { type: Number, required: true },
		},
		data() {
			return { tick: Date.now(), timer: 0 };
		},
		computed: {
			code(): string {
				return ticketCode(this.task.title) ?? '';
			},
			title(): string {
				return stripTicket(this.task.title);
			},
			clock(): string {
				const nowSec = this.tick / 1000 + this.clockOffset;
				return formatClock(runningElapsed(this.task, nowSec));
			},
			openId(): string {
				return actionIds.open(this.task.workspaceId, this.task.id);
			},
			stopId(): string {
				return actionIds.stop(this.task.workspaceId, this.task.id);
			},
		},
		mounted() {
			this.timer = window.setInterval(() => {
				this.tick = Date.now();
			}, 1000);
		},
		beforeUnmount() {
			window.clearInterval(this.timer);
		},
	});
</script>
