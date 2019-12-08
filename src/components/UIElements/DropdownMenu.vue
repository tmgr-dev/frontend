
<template>
    <div class="relative">
        <button @click="isOpen = !isOpen">
            <svg
                class="mr-3 md:mr-1 h-12 w-6 fill-current text-grey-dark"
                xmlns="http://www.w3.org/2000/svg"
                viewBox="0 0 20 20"
            >
                <path
                    d="M4 12a2 2 0 1 1 0-4 2 2 0 0 1 0 4zm6 0a2 2 0 1 1 0-4 2 2 0 0 1 0 4zm6 0a2 2 0 1 1 0-4 2 2 0 0 1 0 4z"
                />
            </svg>
        </button>
        <button v-if="isOpen" @click="isOpen = false" tabindex="-1" class="fixed z-10 inset-0 h-full w-full bg-black opacity-50 cursor-default"></button>
        <div v-if="isOpen" class="absolute right-0 z-10 mt-2 py-2 w-48 bg-white rounded-lg shadow-xl">
            <a href="#" class="block px-4 py-2 text-gray-800 hover:bg-indigo-500 hover:text-white">Open</a>
            <a href="#" class="block px-4 py-2 text-gray-800 hover:bg-indigo-500 hover:text-white">Done</a>
            <a href="#" class="block px-4 py-2 text-gray-800 hover:bg-indigo-500 hover:text-white">Hide</a>
            <a href="#" class="block px-4 py-2 text-gray-800 hover:bg-indigo-500 hover:text-white">Archive</a>
        </div>
    </div>
</template>

<script>
    export default {
        data() {
            return {
                isOpen: false
            }
        },
        created() {
            const handleEscape = (e) => {
                if (e.key === 'Esc' || e.key === 'Escape') {
                    this.isOpen = false
                }
            }
            document.addEventListener('keydown', handleEscape)
            this.$once('hook:beforeDestroy', () => {
                document.removeEventListener('keydown', handleEscape)
            })
        }
    }
</script>
