<template>
    <div>
        <Navbar />
        <header class="container mx-auto px-4 my-5 relative">
            <h1 :class="`text-3xl ${$color('h1')}-800 pt-5`">
                <slot name="header"></slot>
            </h1>
            <slot name="action"></slot>
        </header>
        <div class="container mx-auto px-4 my-5">
            <slot name="body"></slot>
        </div>
        <div class="flex flex-col fixed z-50 right-0 bottom-0 mr-4">
            <slot name="alerts"></slot>
        </div>
        <slot name="footer"></slot>
    </div>
</template>

<script>
    export default {
        name: "BaseLayout"
    }
</script>

<style scoped>

</style>
