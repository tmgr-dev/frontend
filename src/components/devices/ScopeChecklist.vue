<template>
	<ul class="space-y-2">
		<li v-for="scope in scopes" :key="scope">
			<label class="flex cursor-pointer items-start gap-2 text-sm">
				<input
					type="checkbox"
					class="mt-0.5 accent-primary"
					:checked="selected.includes(scope)"
					:disabled="disabled"
					:data-testid="`scope-${scope}`"
					@change="$emit('toggle', scope)"
				/>
				<span>
					<span class="block text-ink">{{ describeScope(scope) }}</span>
					<span class="block font-mono text-xs text-ink-subtle">{{
						scope
					}}</span>
				</span>
			</label>
		</li>
	</ul>
</template>

<script>
	import { describeScope } from '@/utils/deviceLogin';
	import { defineComponent } from 'vue';

	export default defineComponent({
		name: 'ScopeChecklist',
		props: {
			scopes: { type: Array, required: true },
			selected: { type: Array, required: true },
			disabled: { type: Boolean, default: false },
		},
		emits: ['toggle'],
		setup() {
			return { describeScope };
		},
	});
</script>
