<template>
	<div>
		<transition name="fade">
			<div class="fixed alert" v-if="showAlert">
				<div class="flex bg-green-400 max-w-sm">
					<div class="w-16 bg-green-700">
						<div class="flex h-full">
							<svg
								class="svg m-auto"
								:class="{'animate': isCreated}"
								xmlns="http://www.w3.org/2000/svg"
								width="34.930046"
								height="34.747898"
								viewBox="0 0 9.2419075 9.1937147">
								<path
									class="mark"
									d="M 2.7743677,4.2547759 4.5261641,6.0815302 8.8650064,1.8464047"
								/>
								<path
									class="circle"
									d="M 9.4582777,3.8695316 A 4.2357225,4.23736 0 0 1 6.4933734,7.9116835 4.2357225,4.23736 0 0 1 1.7493845,6.2950165 4.2357225,4.23736 0 0 1 1.867668,1.2827869 4.2357225,4.23736 0 0 1 6.6826327,-0.10812404"
									transform="matrix(0.98909829,0.1472568,-0.14747989,0.98906505,0,0)"
								/>
							</svg>
						</div>
					</div>
					<div class="w-auto text-grey-500 items-center p-4 alert__info" :class="{'show': isCreated}">
					<span class="text-lg font-bold pb-4">
						{{ title }}
					</span>
						<p class="leading-tight">
							{{ description }}
						</p>
					</div>
				</div>
			</div>
		</transition>
	</div>
</template>

<script>
	export default {
		name: "Alert",
		props: {
			lifetime: {
				type: Number,
				required: false,
				default: 3000
			},
			autohide: {
				type: Boolean,
				required: false,
				default: true
			}
		},
		data: () => ({
			isCreated: false,
			title: null,
			description: null,
			showAlert: false
		}),
		methods: {
			show(title, description = '', lifetime = null, autohide = null) {
				lifetime = lifetime ?? this.lifetime
				autohide = autohide ?? this.autohide

				this.title = title
				this.description = description

				this.isCreated = false

				setTimeout(() => {
					this.showAlert = true
					setTimeout(() => this.isCreated = true, 1)
					if (autohide) {
						this.hide()
					}
				}, 100)
			},
			hide(lifetime = null) {
				lifetime = lifetime ?? this.lifetime

				setTimeout(() => {
					this.isCreated = false
					this.showAlert = false
				}, lifetime)
			}
		}
	}
</script>

<style scoped lang="scss">
	.alert {
		right: 20px;
		bottom: 20px;
	  color: rgba(26, 32, 44, 0.75);

		&__info {
			opacity: 0;
			transition: .3s 1s;

			&.show {
				opacity: 1;
			}
		}
	}

	.mark, .circle {
		fill: none;
		stroke: #fff;
		stroke-width: 0.720196;
		stroke-linecap: round;
	}

	.mark {
		stroke-dasharray: 10;
		stroke-dashoffset: 10;
		transition: .5s .75s;
	}

	.circle {
		stroke-dasharray: 22;
		stroke-dashoffset: 22;
		transition: .5s .35s;
	}

	.svg.animate {
		.circle, .mark {
			stroke-dashoffset: 0;
		}
	}
</style>
