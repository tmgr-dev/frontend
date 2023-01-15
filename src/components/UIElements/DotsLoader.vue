<template>
	<input v-model="dots" type="text" />
</template>

<script>
	export default {
		name: 'DotsLoader',
		data: () => ({
			dots: '',
			interval: null,
		}),
		created() {
			this.dotsLoad();
		},
		destroyed() {
			clearInterval(this.interval);
		},
		methods: {
			dotsLoad() {
				this.interval = setInterval(() => {
					if (this.dots.length === 3) {
						this.dots = '';
					}

					this.dots += '.';
				}, 170);
			},
		},
	};
</script>

<style lang="scss" scoped>
	input {
		border: none;
		background: transparent;
		position: absolute;
		left: 101%;
		color: #fff;
		width: 100%;
		bottom: 0;

		&:focus,
		&:hover,
		&:active {
			outline: none;
		}
	}
</style>
