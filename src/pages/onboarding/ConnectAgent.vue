<template>
	<div data-testid="connect-agent">
		<div data-tour="connect.client-tabs"></div>
		<div data-tour="connect.command"></div>
		<div data-tour="connect.status"></div>
		<div data-tour="connect.prompt"></div>
	</div>
</template>

<script lang="ts">
import { defineComponent } from 'vue';

export default defineComponent({
	name: 'ConnectAgent',
});
</script>
