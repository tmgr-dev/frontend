<template>
	<!--Nav-->
	<nav
		id="header"
		class="fixed top-0 z-30 w-full text-white"
		style="
			background: rgba(0, 0, 0, 0)
				linear-gradient(to right, rgb(0, 210, 255), rgb(58, 123, 213)) repeat
				scroll 0 0;
		"
	>
		<div
			class="container mx-auto mt-0 flex w-full flex-wrap items-center justify-between py-2"
		>
			<div class="flex items-center pl-4">
				<a
					class="toggleColour text-2xl font-bold text-white no-underline hover:no-underline lg:text-4xl"
					href="#"
				>
					<!--Icon from: http://www.potlabicons.com/ -->
					<svg
						class="inline h-8 fill-current"
						viewBox="0 0 512.005 512.005"
						xmlns="http://www.w3.org/2000/svg"
					>
						<rect
							fill="#2a2a31"
							height="50.502"
							transform="matrix(1,0,0,1,0,0)"
							width="479.767"
							x="16.539"
							y="425.626"
						/>
						<path
							class="plane-take-off"
							d=" M 510.7 189.151 C 505.271 168.95 484.565 156.956 464.365 162.385 L 330.156 198.367 L 155.924 35.878 L 107.19 49.008 L 211.729 230.183 L 86.232 263.767 L 36.614 224.754 L 0 234.603 L 45.957 314.27 L 65.274 347.727 L 105.802 336.869 L 240.011 300.886 L 349.726 271.469 L 483.935 235.486 C 504.134 230.057 516.129 209.352 510.7 189.151 Z "
						/>
					</svg>
					TASK MANAGER
				</a>
			</div>

			<div class="pr-4 lg:hidden">
				<button
					id="nav-toggle"
					class="flex items-center p-1 text-orange-800 hover:text-gray-900"
				>
					<svg
						class="h-6 w-6 fill-current"
						viewBox="0 0 20 20"
						xmlns="http://www.w3.org/2000/svg"
					>
						<title>Menu</title>
						<path d="M0 3h20v2H0V3zm0 6h20v2H0V9zm0 6h20v2H0v-2z" />
					</svg>
				</button>
			</div>

			<div
				id="nav-content"
				class="z-20 mt-2 hidden w-full flex-grow p-4 text-black lg:mt-0 lg:block lg:flex lg:w-auto lg:items-center lg:bg-transparent lg:p-0"
			>
				<ul class="list-reset flex-1 items-center justify-end lg:flex">
					<li class="mr-3">
						<router-link
							v-if="!$store.getters.isLoggedIn"
							class="hover:text-underline inline-block py-2 px-4 text-white no-underline hover:text-gray-200"
							href="#"
							to="/login"
						>
							Login
						</router-link>
						<router-link
							v-else
							class="hover:text-underline inline-block py-2 px-4 text-white no-underline hover:text-gray-200"
							href="#"
							to="/"
						>
							Go to system
						</router-link>
					</li>
				</ul>
				<router-link
					v-if="!$store.getters.isLoggedIn"
					id="navAction"
					class="mx-auto mt-4 rounded-full bg-white py-4 px-8 font-bold text-gray-800 opacity-75 shadow hover:underline lg:mx-0 lg:mt-0"
					to="/register"
				>
					Register
				</router-link>
			</div>
		</div>

		<hr class="my-0 border-b border-gray-100 py-0 opacity-25" />
	</nav>
</template>

<script>
	export default {
		name: 'MainNav',
	};
</script>
