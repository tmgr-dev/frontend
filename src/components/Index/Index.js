import MainNav from './modules/MainNav';
import MainFeatures from './modules/MainFeatures';
import MainHeader from './modules/MainHeader';
import MainTwitter from './modules/MainTwitter';
import MainPricing from './modules/MainPricing';
import MainFooter from './modules/MainFooter';

export default {
	name: 'Index',
	components: {
		MainNav,
		MainFeatures,
		MainHeader,
		MainTwitter,
		MainPricing,
		MainFooter,
	},
	props: [],
	data() {
		return {
			scrollHandler: null,
		};
	},
	computed: {},
	mounted() {
		if (!document.location.pathname) {
			const header = document.getElementById('header');
			const navcontent = document.getElementById('nav-content');
			const navaction = document.getElementById('navAction');
			const toToggle = document.querySelectorAll('.toggleColour');

			this.scrollHandler = () => {
				const scrollpos = window.scrollY;
				if (scrollpos > 10) {
					header?.classList.add('bg-white');
					navaction?.classList.remove('bg-white');
					navaction?.classList.add('gradient');
					navaction?.classList.remove('text-gray-800');
					navaction?.classList.add('text-white');
					for (let i = 0; i < toToggle.length; i++) {
						toToggle[i].classList.add('text-gray-800');
						toToggle[i].classList.remove('text-white');
					}
					header?.classList.add('shadow');
					navcontent?.classList.remove('bg-gray-100');
					navcontent?.classList.add('bg-white');
				} else {
					header?.classList.remove('bg-white');
					navaction?.classList.remove('gradient');
					navaction?.classList.add('bg-white');
					navaction?.classList.remove('text-white');
					navaction?.classList.add('text-gray-800');
					for (let i = 0; i < toToggle.length; i++) {
						toToggle[i].classList.add('text-white');
						toToggle[i].classList.remove('text-gray-800');
					}
					header?.classList.remove('shadow');
					navcontent?.classList.remove('bg-white');
					navcontent?.classList.add('bg-gray-100');
				}
			};

			document.addEventListener('scroll', this.scrollHandler);
		}
	},
	beforeUnmount() {
		if (this.scrollHandler) {
			document.removeEventListener('scroll', this.scrollHandler);
		}
	},
	methods: {},
};
