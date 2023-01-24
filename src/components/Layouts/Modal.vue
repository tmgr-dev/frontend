<template>
	<div class="overlay fixed inset-0 z-50 flex" :data-name="name" @click="close">
		<div
			class="modal bg-white dark:bg-gray-900 rounded-lg m-auto"
			:class="modalClass"
		>
			<slot name="modal-body"></slot>
		</div>
	</div>
</template>

<script>
	export default {
		name: 'Modal',
		emits: ['close'],
		props: {
			name: {
				type: String,
				required: false,
			},
			closeOnBgClick: {
				type: Boolean,
				required: false,
				default: true,
			},
			modalClass: {
				type: String,
				required: false,
			},
		},
		methods: {
			close(e) {
				if (this.closeOnBgClick) {
					if (
						e.target.classList.contains('overlay') &&
						e.target.dataset.name === this.name
					) {
						this.$emit('close');
					}
				}
			},
		},
	};
</script>

<style lang="scss" scoped>
	.overlay {
		background: rgba(0, 0, 0, 0.555);
	}

	.modal {
		max-width: 95%;
		max-height: 95%;

		nav {
			display: none;
		}
	}
</style>
