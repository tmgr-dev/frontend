<template>
	<section
		class="h-screen w-screen overflow-hidden flex items-center justify-center text-neutral-600"
	>
		<div class="bg-blue-400 w-full h-full">
			<div
				class="flex flex-col items-center flex-1 h-full justify-center px-4 sm:px-0"
			>
				<div
					class="flex rounded-lg shadow-lg w-full sm:w-3/4 lg:w-1/2 bg-white sm:mx-0"
					style="height: 500px"
				>
					<div class="flex flex-col w-full md:w-1/2 p-4">
						<div class="flex flex-col flex-1 justify-center mb-8">
							<h1 class="text-4xl text-center font-thin">
								<slot name="title" />
							</h1>

							<div class="w-full mt-4">
								<slot name="body" />

								<div class="text-center mt-4">
									<slot name="footer" />
								</div>
							</div>
						</div>
					</div>

					<div
						class="hidden md:block md:w-1/2 rounded-r-lg bg-[url(../../assets/img/bg-login.jpeg)] bg-center bg-cover bg-no-repeat"
					/>
				</div>
			</div>
		</div>
	</section>
</template>
