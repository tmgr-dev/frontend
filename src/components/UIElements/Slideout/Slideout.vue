<template>
	<div>
		<slot></slot>
	</div>
</template>
<script>
import SlideoutJS from 'slideout';

const events = ['beforeclose', 'close', 'beforeopen', 'open', 'translatestart', 'translate', 'translateend'];

export default {
	data() {
		return {
			slideout: null,
			waitTimeout: null
		};
	},
	props: {
		panel: {},
		menu: {},
		padding: { default: 256 },
		tolerance: { default: 70 },
		touch: { default: true },
		easing: { default: 'ease' },
		side: { default: 'left' },
		duration: { default: 300 },
		toggleSelectors: {
			default: function() {
				return [];
			}
		}
	},
	emits: [
		...events.map(event => `on-${event}`),
		...events.map(event => `once-${event}`)
	],
	name: 'Slideout',
	mounted: function() {
		this.wait(() => {
			this.$store.commit('setSlideout', new SlideoutJS({
				'panel': document.querySelector(this.panel),
				'menu': document.querySelector(this.menu),
				'padding': this.padding,
				'tolerance': this.tolerance,
				'touch': this.touch,
				'easing': this.easing,
				'side': this.side,
				'duration': this.duration
			}));
			this.toggleSelectors.forEach(selector => {
				document.querySelectorAll(selector).forEach(element => {
					element.addEventListener('click', () => {
						this.$store.getters.slideout.toggle();
					});
				});
			});
			events.forEach(event => {
				this.$store.getters.slideout.on(event, (data) => {
					this.$emit('on-' + event, data);
				});
				this.$store.getters.slideout.once(event, (data) => {
					this.$emit('once-' + event, data);
				});
			});
		});
	},
	methods: {
		wait(cb) {
			if (!document.querySelector(this.panel)) {
				this.waitTimeout = setTimeout(() => {
					return this.wait(cb);
				}, 0);
				return;
			}
			cb();
		}
	}
};

</script>

<style>
body {
	width: 100%;
	height: 100%;
	margin: 0;
}

.slideout-menu {
	position: fixed;
	top: 0;
	bottom: 0;
	width: 256px;
	height: 100%;
	overflow-y: scroll;
	-webkit-overflow-scrolling: touch;
	z-index: 0;
	display: none;
}

.slideout-menu-left {
	left: 0;
}

.slideout-menu-right {
	right: 0;
}

.slideout-panel {
	position: relative;
	z-index: 1;
	/*	will-change: transform;*/
	min-height: 100vh;
}

.slideout-open,
.slideout-open body,
.slideout-open .slideout-panel {
	overflow: hidden;
}

.slideout-open .slideout-menu {
	display: block;
}
</style>
