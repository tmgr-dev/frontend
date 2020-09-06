<template>
	<input type="text" v-model="dots">
</template>

<script>
	export default {
		name: 'DotsLoader',
		data: () => ({
			dots: '',
			interval: null
		}),
		methods: {
			dotsLoad () {
				this.interval = setInterval(() => {
					if (this.dots.length === 3) {
						this.dots = ''
					}

					this.dots += '.'
				}, 170)
			}
		},
		created () {
			this.dotsLoad()
		},
		destroyed() {
			clearInterval(this.interval)
		}
	}
</script>

<style scoped lang="scss">
	input {
		border: none;
		background: transparent;
		position: absolute;
		left: 101%;
		color: #fff;
		width: 100%;
		bottom: 0;

		&:focus, &:hover, &:active {
			outline: none;
		}
	}
</style>
