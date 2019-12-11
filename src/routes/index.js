import Login from '../components/Auth/Login'
import Register from '../components/Auth/Register'
import ForgetPassword from '../components/Auth/ForgetPassword'
import ResetPassword from '../components/Auth/ResetPassword'
import Index from '../components/Index'
import ProjectCategoryForm from '../components/ProjectsCategories/ProjectCategoryForm'
import ProjectCategoryList from '../components/ProjectsCategories/ProjectCategoryList'
import ProjectCategoryView from '../components/ProjectsCategories/ProjectCategoryView'
import TasksList from "../components/Tasks/TasksList";

const routes = [
    {
        path: '/',
        component: Index,
        name: Index.name
    },
    {
        path: '/register',
        component: Register,
        name: Register.name
    },
    {
        path: '/password/forget',
        component: ForgetPassword,
        name: ForgetPassword.name
    },
    {
        path: '/password/reset',
        component: ResetPassword,
        name: ResetPassword.name
    },
    {
        path: '/login',
        component: Login,
        name: Login.name
    },
    {
        path: '/projects-categories',
        component: ProjectCategoryList,
        name: ProjectCategoryList.name
    },
    {
        path: '/projects-categories/view',
        component: ProjectCategoryView,
        name: ProjectCategoryView.name
    },
    {
        path: '/projects-categories/form',
        component: ProjectCategoryForm,
        name: ProjectCategoryForm.name
    },
    {
        path: '/tasks',
        component: TasksList,
        name: 'Current' + TasksList.name
    },
    {
        path: '/tasks/hidden',
        component: TasksList,
        name: 'Hidden' + TasksList.name
    },
    {
        path: '/tasks/archive',
        component: TasksList,
        name: 'Archive' + TasksList.name
    },
]

export default routes
