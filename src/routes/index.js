import { createRouter, createWebHistory } from 'vue-router'
import store from '../store'

const ProjectCategoryList = () => import('@/components/ProjectsCategories/ProjectCategoryList')
const TasksForm = () => import('@/components/Tasks/TasksForm')
const TasksList = () => import('@/components/Tasks/TasksList')
const ProjectCategoryForm = () => import('@/components/ProjectsCategories/ProjectCategoryForm')

let router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      component: () => import('@/components/Index/index.vue'),
      name: 'Index'
    },
    {
      path: '/register',
      component: () => import('@/components/Auth/Register'),
      name: 'Register',
      meta: {
        allowedGuests: true,
        transitionName: 'fade-fast'
      }
    },
    {
      path: '/password/forget',
      component: () => import('@/components/Auth/ForgetPassword'),
      name: 'ForgetPassword',
      meta: {
        allowedGuests: true,
        transitionName: 'fade-fast'
      }
    },
    {
      path: '/password/reset',
      component: () => import('@/components/Auth/ResetPassword'),
      name: 'ResetPassword',
      meta: {
        allowedGuests: true,
        transitionName: 'fade-fast'
      }
    },
    {
      path: '/login',
      component: () => import('@/components/Auth/Login'),
      name: 'Login',
      meta: {
        allowedGuests: true,
        transitionName: 'fade-fast'
      }
    },
    {
      path: '/projects-categories',
	    meta: {
		    transitionName: 'slide',
		    navbarHidden: true
	    },
	    component: ProjectCategoryList,
	    name: 'ProjectCategoryList',
    },
	  {
		  path: '/projects-categories/create',
		  meta: {
			  transitionName: 'slide',
			  navbarHidden: true
		  },
		  component: ProjectCategoryForm,
		  name: 'ProjectCategoryCreate'
	  },
	  {
		  path: '/projects-categories/:id/edit',
		  meta: {
			  transitionName: 'slide',
			  navbarHidden: true
		  },
		  component: ProjectCategoryForm,
		  name: 'ProjectCategoryEdit'
	  },
    {
      path: '/projects-categories/status/:status?',
      meta: {
        transitionName: 'slide',
        navbarHidden: true
      },
      component: ProjectCategoryList,
      name: 'ProjectCategoryChildrenListWithStatus'
    },
    {
      path: '/projects-categories/:id/children/:status?',
      meta: {
        transitionName: 'slide',
        navbarHidden: true
      },
      component: ProjectCategoryList,
      name: 'ProjectCategoryChildrenList'
    },
    {
      path: '/projects-categories/:project_category_id/create',
      meta: {
        transitionName: 'slide',
        navbarHidden: true
      },
      component: ProjectCategoryForm,
      name: 'ProjectCategoryCreateInCategory'
    },
	  {
		  path: '/project-categories/:project_category_id/tasks/create',
		  meta: {
			  transitionName: 'slide',
			  navbarHidden: true
		  },
		  component: TasksForm,
		  name: 'TasksCreateWithProjectCategoryId'
	  },
	  {
		  path: '/tasks/create',
		  meta: {
			  transitionName: 'slide',
			  navbarHidden: true
		  },
		  component: TasksForm,
		  name: 'TasksCreate'
	  },
	  {
		  path: '/tasks/:status?',
		  component: TasksList,
		  meta: {
			  transitionName: 'slide',
			  navbarHidden: true
		  },
		  name: 'Tasks'
	  },
	  {
		  path: '/tasks/:id/edit',
		  meta: {
			  transitionName: 'slide',
			  navbarHidden: true
		  },
		  component: TasksForm,
		  name: 'TasksEdit'
	  },
    {
      path: '/settings',
      component: () => import('@/components/Settings'),
      meta: {
        transitionName: 'fade-fast',
        navbarHidden: true
      },
      name: 'Settings'
    }
  ]
})

router.beforeEach((to, from, next) => {
  if (to.matched.some(record => record.meta.allowedGuests) && store.getters.isLoggedIn) {
    next({name: 'CurrentTasksList'})
  }
  if (to.matched.some(record => !record.meta.allowedGuests)) {
    if (!store.getters.isLoggedIn) {
      next({name: 'Login'})
    } else {
      next()
    }
  } else {
    next()
  }
})

export default router
