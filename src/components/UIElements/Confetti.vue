<script>
import confetti from 'canvas-confetti';

export default {
	name: 'Confetti',
	data: () => ({
		end: Date.now() + (500)
		// colors: ['#E94B3C', '#ECDB54', '#6F9FD8', '#EC9787']
	}),
	methods: {
		leftConfetti() {
			confetti({
				particleCount: 5,
				angle: 30,
				spread: 100,
				origin: { x: 0 }
			});
		},
		rightConfetti() {
			confetti({
				particleCount: 5,
				angle: 150,
				spread: 100,
				origin: { x: 1 }
			});
		},
		letsGo() {
			if (document.documentElement.clientWidth <= 500) {
				this.leftConfetti();
			} else {
				this.leftConfetti();
				this.rightConfetti();
			}

			if (Date.now() < this.end) {
				requestAnimationFrame(this.letsGo);
			}
		}
	},
	mounted() {
		/*confetti({
			particleCount: 1000,
			spread: 150,
			gravity: 2.2,
			startVelocity: 130,
			origin: {
				y: 0.9
			}
		})*/
		this.letsGo();
	}
};
</script>
