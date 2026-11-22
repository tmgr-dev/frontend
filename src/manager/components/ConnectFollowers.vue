<template>
	<div v-if="followers > 0" class="mgr-followers">
		<button
			type="button"
			class="btn sm"
			:disabled="busy"
			@click="$emit('connect')"
		>
			{{
				busy
					? $t('manager.followers.connecting')
					: $t('manager.followers.connect', { count: pending })
			}}
		</button>
		<p v-for="name in hints" :key="`hint-${name}`" class="mgr-followers-line">
			{{ $t('manager.followers.scopeHint', { name }) }}
		</p>
		<p
			v-for="(r, i) in results"
			:key="`${r.follower}-${i}`"
			class="mgr-followers-line"
			role="status"
		>
			{{ line(r) }}
		</p>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import type { FollowerResult } from '../types';

	export default defineComponent({
		name: 'ManagerConnectFollowers',
		props: {
			followers: { type: Number, required: true },
			pending: { type: Number, required: true },
			busy: { type: Boolean, default: false },
			hints: { type: Array as PropType<string[]>, default: () => [] },
			results: { type: Array as PropType<FollowerResult[]>, default: () => [] },
		},
		emits: ['connect'],
		methods: {
			line(r: FollowerResult): string {
				const message = r.message ?? '';
				if (!r.follower)
					return this.$t('manager.followers.failed', { message });
				const known = ['enrolled', 'already', 'refused', 'unreachable'];
				if (known.includes(r.state))
					return this.$t(`manager.followers.${r.state}`, { name: r.follower });
				return this.$t('manager.followers.error', {
					name: r.follower,
					message,
				});
			},
		},
	});
</script>
