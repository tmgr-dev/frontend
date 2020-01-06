import Vue from 'vue'
import Router from 'vue-router'

Vue.use(Router)

export default new Router({
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
            name: 'Register'
        },
        {
            path: '/password/forget',
            component: () => import('@/components/Auth/ForgetPassword'),
            name: 'ForgetPassword'
        },
        {
            path: '/password/reset',
            component: () => import('@/components/Auth/ResetPassword'),
            name: 'ResetPassword'
        },
        {
            path: '/login',
            component: () => import('@/components/Auth/Login'),
            name: 'Login'
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
