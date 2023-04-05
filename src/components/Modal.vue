<template>
	<div
		class="fixed inset-0 z-50 flex bg-black/50"
		:data-name="name"
		@click="close"
	>
		<div
			class="m-auto max-h-[95%] max-w-[95%] rounded-lg bg-white dark:bg-gray-900"
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
		data() {
			return {
				initialUrl: location.href,
			};
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
			closeByEsc(e) {
				if (e.key === 'Escape') {
					this.$emit('close');
				}
			},
		},
		mounted() {
			document.body.addEventListener('keydown', this.closeByEsc);
		},
		unmounted() {
			document.body.removeEventListener('keydown', this.closeByEsc);
			if (location.href !== this.initialUrl) {
				history.pushState({}, '', this.initialUrl);
			}
		},
	};
</script>
