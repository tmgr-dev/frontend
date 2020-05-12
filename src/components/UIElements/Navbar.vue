<template>
    <nav :class="`${$color('bgNav')} shadow`" role="navigation">
        <div class="container mx-auto p-4 flex flex-wrap items-center md:flex-no-wrap">
            <div class="mr-4 md:mr-8">
                <router-link to="/tasks" rel="home">
                    Task Manager
                </router-link>
            </div>
            <div class="ml-auto md:hidden">
                <button class="flex items-center px-3 py-2 border rounded" type="button" @click="isHidden = !isHidden">
                    <svg class="h-3 w-3" viewBox="0 0 20 20" xmlns="http://www.w3.org/2000/svg">
                        <title>Menu</title>
                        <path d="M0 3h20v2H0V3zm0 6h20v2H0V9zm0 6h20v2H0v-2z"/>
                    </svg>
                </button>
            </div>
            <div class="w-full md:w-auto md:flex-grow md:flex md:items-center md:block" :class="{ hidden: isHidden }">
                <ul class="flex flex-col mt-4 -mx-4 pt-4 border-t md:flex-row md:items-center md:mx-0 md:mt-0 md:pt-0 md:mr-4 lg:mr-8 md:border-0">
                    <li>
                        <router-link to="/projects-categories" :class="`block px-4 py-1 md:p-2 lg:px-4 ${$route.path === '/projects-categories' ? $color('navLinkFocused') + '-600' : ''}`" href="#" title="Link">Categories</router-link>
                    </li>
                    <li>
                        <router-link to="/tasks" :class="`block px-4 py-1 md:p-2 lg:px-4 ${$route.path === '/tasks' ? $color('navLinkFocused') + '-600' : ''}`" href="#" title="Link">Current tasks</router-link>
                    </li>
                    <li>
                        <router-link to="/tasks/hidden" :class="`block px-4 py-1 md:p-2 lg:px-4 ${$route.path === '/tasks/hidden' ? $color('navLinkFocused') + '-600' : ''}`" href="#" title="Link">Hidden</router-link>
                    </li>
                    <li>
                        <router-link to="/tasks/archive" :class="`block px-4 py-1 md:p-2 lg:px-4 ${$route.path === '/tasks/archive' ? $color('navLinkFocused') + '-600': ''}`" href="#" title="Link">Archive</router-link>
                    </li>
                </ul>
                <AccountDropdown />
            </div>
        </div>
    </nav>
</template>

<script>
    export default {
        name: 'Navbar',
        data() {
            return {
                isHidden: false,
            }
        }        
    }
</script>
