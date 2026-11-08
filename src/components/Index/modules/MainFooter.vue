<template>
	<!--Footer-->
	<footer class="bg-white">
		<div class="container mx-auto px-8">
			<div class="flex w-full flex-col py-6 md:flex-row">
				<div class="mb-6 flex-1">
					<a
						class="text-2xl font-bold text-orange-600 no-underline hover:no-underline lg:text-4xl"
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
						{{ $t('public.index.footer.brand') }}
					</a>
				</div>

				<div class="flex-1">
					<p class="text-gray-500 uppercase md:mb-6">
						{{ $t('public.index.footer.links') }}
					</p>
					<ul class="list-reset mb-6">
						<li class="mt-2 mr-2 inline-block md:mr-0 md:block">
							<a
								class="text-gray-800 no-underline hover:text-orange-500 hover:underline"
								href="#"
								>{{ $t('public.index.footer.faq') }}</a
							>
						</li>
						<li class="mt-2 mr-2 inline-block md:mr-0 md:block">
							<a
								class="text-gray-800 no-underline hover:text-orange-500 hover:underline"
								href="#"
								>{{ $t('public.index.footer.help') }}</a
							>
						</li>
						<li class="mt-2 mr-2 inline-block md:mr-0 md:block">
							<a
								class="text-gray-800 no-underline hover:text-orange-500 hover:underline"
								href="#"
								>{{ $t('public.index.footer.support') }}</a
							>
						</li>
					</ul>
				</div>
				<div class="flex-1">
					<p class="text-gray-500 uppercase md:mb-6">
						{{ $t('public.index.footer.legal') }}
					</p>
					<ul class="list-reset mb-6">
						<li class="mt-2 mr-2 inline-block md:mr-0 md:block">
							<a
								class="text-gray-800 no-underline hover:text-orange-500 hover:underline"
								href="#"
								>{{ $t('public.index.footer.terms') }}</a
							>
						</li>
						<li class="mt-2 mr-2 inline-block md:mr-0 md:block">
							<a
								class="text-gray-800 no-underline hover:text-orange-500 hover:underline"
								href="#"
								>{{ $t('public.index.footer.privacy') }}</a
							>
						</li>
					</ul>
				</div>
				<div class="flex-1">
					<p class="text-gray-500 uppercase md:mb-6">
						{{ $t('public.index.footer.social') }}
					</p>
					<ul class="list-reset mb-6">
						<li class="mt-2 mr-2 inline-block md:mr-0 md:block">
							<a
								class="text-gray-800 no-underline hover:text-orange-500 hover:underline"
								href="#"
								>{{ $t('public.index.footer.facebook') }}</a
							>
						</li>
						<li class="mt-2 mr-2 inline-block md:mr-0 md:block">
							<a
								class="text-gray-800 no-underline hover:text-orange-500 hover:underline"
								href="#"
								>{{ $t('public.index.footer.linkedin') }}</a
							>
						</li>
						<li class="mt-2 mr-2 inline-block md:mr-0 md:block">
							<a
								class="text-gray-800 no-underline hover:text-orange-500 hover:underline"
								href="#"
								>{{ $t('public.index.footer.twitter') }}</a
							>
						</li>
					</ul>
				</div>
				<div class="flex-1">
					<p class="text-gray-500 uppercase md:mb-6">
						{{ $t('public.index.footer.company') }}
					</p>
					<ul class="list-reset mb-6">
						<li class="mt-2 mr-2 inline-block md:mr-0 md:block">
							<a
								class="text-gray-800 no-underline hover:text-orange-500 hover:underline"
								href="#"
								>{{ $t('public.index.footer.blog') }}</a
							>
						</li>
						<li class="mt-2 mr-2 inline-block md:mr-0 md:block">
							<a
								class="text-gray-800 no-underline hover:text-orange-500 hover:underline"
								href="#"
								>{{ $t('public.index.footer.about') }}</a
							>
						</li>
						<li class="mt-2 mr-2 inline-block md:mr-0 md:block">
							<a
								class="text-gray-800 no-underline hover:text-orange-500 hover:underline"
								href="#"
								>{{ $t('public.index.footer.contact') }}</a
							>
						</li>
					</ul>
				</div>
			</div>
		</div>

		<a href="https://www.freepik.com/free-photos-vectors/background">{{
			$t('public.index.footer.freepik')
		}}</a>
		<a href="https://github.com/tailwindtoolbox/Landing-Page">{{
			$t('public.index.footer.template')
		}}</a>
	</footer>
</template>

<script>
	export default {
		name: 'MainFooter',
	};
</script>
