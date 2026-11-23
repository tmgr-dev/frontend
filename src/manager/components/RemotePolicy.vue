<template>
	<div v-if="leaders.length" class="mgr-policy">
		<label
			v-for="l in leaders"
			:key="l.machine"
			:title="$t('manager.remotePolicy.hint')"
		>
			<input
				type="checkbox"
				:checked="l.ask"
				:disabled="busy || !l.ask"
				@change="onChange(l.machine, $event)"
			/>
			<span>{{
				$t('manager.remotePolicy.allowAsk', { name: nameOf(l.machine) })
			}}</span>
		</label>
		<p
			v-for="l in leaders.filter((x) => !x.ask)"
			:key="`on-${l.machine}`"
			class="mgr-followers-line"
		>
			{{ $t('manager.remotePolicy.turnOn', { machine: l.machine }) }}
		</p>
		<p v-if="error" class="mgr-followers-line" role="alert">
			{{ $t('manager.remotePolicy.failed', { message: error }) }}
		</p>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import type { ManagerMachine, RemotePolicyLeader } from '../types';

	export default defineComponent({
		name: 'ManagerRemotePolicy',
		props: {
			leaders: {
				type: Array as PropType<RemotePolicyLeader[]>,
				default: () => [],
			},
			machines: {
				type: Array as PropType<ManagerMachine[]>,
				default: () => [],
			},
			busy: { type: Boolean, default: false },
			error: { type: String, default: '' },
		},
		emits: ['toggle'],
		methods: {
			onChange(machine: string, ev: Event) {
				this.$emit('toggle', machine, (ev.target as HTMLInputElement).checked);
			},
			nameOf(machine: string): string {
				return (
					this.machines.find((m) => m.machine === machine)?.label || machine
				);
			},
		},
	});
</script>
