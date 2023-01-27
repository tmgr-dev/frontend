<template>
	<div ref="modalOverlay" class="overlay fixed inset-0 z-50 flex" :data-name="name" @click="close">
		<div
			class="modal bg-white dark:bg-gray-900 rounded-lg m-auto"
			:class="modalClass"
			ref="modal"
			@click.prevent="() => {}"
		>
			<slot name="modal-body"></slot>
		</div>
	</div>
</template>

<script>
	import { TimelineMax, TweenMax, TweenLite } from 'gsap';

	export default {
		name: 'Modal',
		emits: ['close', 'show'],
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
		data () {
			return {
				hidden: true
			}
		},
		mounted() {
			TweenMax.set([this.$refs.modalOverlay, this.$refs.modal], { autoAlpha: 0 });
		},
		methods: {
			show(e) {
				this.hidden = false
				const modal = this.$refs.modal
				const modalOverlay = this.$refs.modalOverlay
				const newRect = this.getPosition(modal, e.target);

				TweenMax.set(modal, {
					x: newRect.left,
					y: newRect.top,
					width: newRect.width,
					height: newRect.height
				});

				const show = new TimelineMax();

				show.to(modalOverlay, 0.2, { autoAlpha: 1 });
				show.to(modal, 0.2, {
					x: 0,
					y: 0,
					width: window.innerWidth - (window.innerWidth * 0.05),
					height: window.innerHeight - (window.innerHeight * 0.05),
					autoAlpha: 1
				});
			},
			close(e) {
				if (
					e &&
					e.target.classList.contains('overlay') &&
					e.target.dataset.name === this.name
				) {
					this.forceClose()
					this.$emit('close');
				}
			},
			forceClose () {
				TweenMax.set([this.$refs.modalOverlay, this.$refs.modal], { autoAlpha: 0 });
			},
			getPosition(elem, target) {
				var targetRect = target.getBoundingClientRect();
				var elemRect = elem.getBoundingClientRect();

				TweenLite.set(elem, {
					x: 0,
					y: 0,
					width: targetRect.width,
					height: targetRect.height
				});
				var newRect = elem.getBoundingClientRect();
				TweenLite.set(elem, { width: elemRect.width, height: elemRect.height });
				return {
					left: targetRect.left - newRect.left,
					top: targetRect.top - newRect.top,
					width: newRect.width,
					height: newRect.height
				};
			}
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
