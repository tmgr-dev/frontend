<template>
	<div class="inline field">
		<label>{{ label }}</label>
		<the-mask v-model="editableValue" :mask="mask" :masked="masked" :placeholder="placeholder" :tokens="tokens"
							:type="type || 'tel'"></the-mask>
		<span>{{ editableValue }}</span>
	</div>
</template>

<script>
import TheMask from '../component.vue';

export default {
	components: { TheMask },
	props: ['label', 'mask', 'placeholder', 'masked', 'type', 'tokens', 'value'],
	data() {
		return {
			editableValue: this.value
		};
	}
};
</script>

<style lang="css">
.ui.form .inline.field label {
	width: 90px;
}
</style>
