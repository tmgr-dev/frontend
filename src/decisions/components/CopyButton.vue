<template>
	<button
		type="button"
		class="md-copy msg-copy"
		:class="state"
		:aria-label="$t('decisions.copy.label')"
		:title="$t('decisions.copy.label')"
		:data-done="$t('decisions.copy.done')"
		:data-failed="$t('decisions.copy.failed')"
		@click="copy"
	>
		<svg
			viewBox="0 0 16 16"
			width="14"
			height="14"
			fill="none"
			stroke="currentColor"
			stroke-width="1.4"
			aria-hidden="true"
		>
			<rect x="5.5" y="5.5" width="8" height="8" rx="1.5" />
			<path
				d="M10.5 5.5v-2a1.5 1.5 0 0 0-1.5-1.5H4A1.5 1.5 0 0 0 2.5 3.5V9A1.5 1.5 0 0 0 4 10.5h1.5"
			/>
		</svg>
	</button>
</template>

<script lang="ts">
	import { defineComponent } from 'vue';
	import { copyText } from '../copy';

	const FEEDBACK_MS = 1500;

	export default defineComponent({
		props: { text: { type: String, required: true } },
		data() {
			return { state: '', timer: 0 as number };
		},
		beforeUnmount() {
			window.clearTimeout(this.timer);
		},
		methods: {
			async copy() {
				this.state = (await copyText(this.text)) ? 'done' : 'failed';
				window.clearTimeout(this.timer);
				this.timer = window.setTimeout(() => (this.state = ''), FEEDBACK_MS);
			},
		},
	});
</script>
