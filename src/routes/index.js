import Vue from 'vue'
import Router from 'vue-router'
import store from '../store'

Vue.use(Router)

let router = new Router({
    mode: 'history',
    base: process.env.BASE_URL,
    routes: [
        {
            path: '/',
            component: () => import('@/components/Index'),
            name: 'Index'
        },
        {
            path: '/register',
            component: () => import('@/components/Auth/Register'),
            name: 'Register',
            meta: {
                allowedGuests: true
            }
        },
        {
            path: '/password/forget',
            component: () => import('@/components/Auth/ForgetPassword'),
            name: 'ForgetPassword',
            meta: {
                allowedGuests: true
            }
        },
        {
            path: '/password/reset',
            component: () => import('@/components/Auth/ResetPassword'),
            name: 'ResetPassword',
            meta: {
                allowedGuests: true
            }
        },
        {
            path: '/login',
            component: () => import('@/components/Auth/Login'),
            name: 'Login',
            meta: {
                allowedGuests: true
            }
        },
        {
            path: '/projects-categories',
            component: () => import('@/components/ProjectsCategories/ProjectCategoryList'),
            name: 'ProjectCategoryList'
        },
        {
            path: '/projects-categories/view',
            component: () => import('@/components/ProjectsCategories/ProjectCategoryView'),
            name: 'ProjectCategoryView'
        },
        {
            path: '/projects-categories/form',
            component: () => import('@/components/ProjectsCategories/ProjectCategoryForm'),
            name: 'ProjectCategoryForm'
        },
        {
            path: '/tasks',
            component: () => import('@/components/Tasks/TasksList'),
            name: 'CurrentTasksList'
        },
        {
            path: '/tasks/form',
            component: () => import('@/components/Tasks/TasksForm'),
            name: 'TasksForm'
        },
        {
            path: '/tasks/hidden',
            component: () => import('@/components/Tasks/TasksList'),
            name: 'HiddenTasksList'
        },
        {
            path: '/tasks/archive',
            component: () => import('@/components/Tasks/TasksList'),
            name: 'ArchiveTasksList'
        },
    ]
})

router.beforeEach((to, from, next) => {
    if (to.matched.some(record => record.meta.allowedGuests) && store.getters.isLoggedIn) {
        next({name: 'CurrentTasksList'})
    }

    if (to.matched.some(record => !record.meta.allowedGuests)) {
        if (!store.getters.isLoggedIn) {
            next({ name: 'Login' })
        } else {
            next()
        }
    } else {
        next()
    }
})

export default router
