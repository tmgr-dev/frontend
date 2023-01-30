<template>
	<!--Nav-->
	<nav
		id="header"
		class="fixed w-full z-30 top-0 text-white"
		style="
			background: rgba(0, 0, 0, 0)
				linear-gradient(to right, rgb(0, 210, 255), rgb(58, 123, 213)) repeat
				scroll 0 0;
		"
	>
		<div
			class="w-full container mx-auto flex flex-wrap items-center justify-between mt-0 py-2"
		>
			<div class="pl-4 flex items-center">
				<a
					class="toggleColour text-white no-underline hover:no-underline font-bold text-2xl lg:text-4xl"
					href="#"
				>
					<!--Icon from: http://www.potlabicons.com/ -->
					<svg
						class="h-8 fill-current inline"
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

			<div class="lg:hidden pr-4">
				<button
					id="nav-toggle"
					class="flex items-center p-1 text-orange-800 hover:text-gray-900"
				>
					<svg
						class="fill-current h-6 w-6"
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
				class="w-full flex-grow lg:flex lg:items-center lg:w-auto hidden lg:block mt-2 lg:mt-0 lg:bg-transparent text-black p-4 lg:p-0 z-20"
			>
				<ul class="list-reset lg:flex justify-end flex-1 items-center">
					<li class="mr-3">
						<router-link
							v-if="!$store.getters.isLoggedIn"
							class="inline-block text-white no-underline hover:text-gray-200 hover:text-underline py-2 px-4"
							href="#"
							to="/login"
						>
							Login
						</router-link>
						<router-link
							v-else
							class="inline-block text-white no-underline hover:text-gray-200 hover:text-underline py-2 px-4"
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
					class="mx-auto lg:mx-0 hover:underline bg-white text-gray-800 font-bold rounded-full mt-4 lg:mt-0 py-4 px-8 shadow opacity-75"
					to="/register"
				>
					Register
				</router-link>
			</div>
		</div>

		<hr class="border-b border-gray-100 opacity-25 my-0 py-0" />
	</nav>
</template>

<script>
	export default {
		name: 'MainNav',
	};
</script>
