<template>
	<div class="pane">
		<div v-if="checks.length === 0" class="placeholder">
			{{ $t('decisions.checks.empty') }}
		</div>
		<div v-for="(check, index) in checks" :key="index" class="check">
			<span class="ic" :class="check.state">{{ GLYPH[check.state] }}</span>
			{{ check.text }}
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import type { CardCheck } from '../types';

	export default defineComponent({
		name: 'ChecksPane',
		props: {
			checks: { type: Array as PropType<CardCheck[]>, required: true },
		},
		data() {
			return { GLYPH: { ok: '✓', bad: '✗', todo: '◷' } };
		},
	});
</script>
