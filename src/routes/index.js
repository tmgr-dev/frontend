import Login from '../components/Auth/Login'
import Register from '../components/Auth/Register'
import ForgetPassword from '../components/Auth/ForgetPassword'
import ResetPassword from '../components/Auth/ResetPassword'
import Index from '../components/Index'
import CategoryForm from '../components/Categories/CategoryForm'
import CategoryList from '../components/Categories/CategoryList'
import CategoryView from '../components/Categories/CategoryView'
import TasksList from "../components/Tasks/TasksList";

const routes = [
    {
        path: '/',
        component: Index
    },
    {
        path: '/register',
        component: Register
    },
    {
        path: '/password/forget',
        component: ForgetPassword
    },
    {
        path: '/password/reset',
        component: ResetPassword
    },
    {
        path: '/login',
        component: Login
    },
    {
        path: '/categories',
        component: CategoryList
    },
    {
        path: '/categories/view',
        component: CategoryView
    },
    {
        path: '/categories/form',
        component: CategoryForm
    },
    {
        path: '/tasks',
        component: TasksList
    },
]

export default routes
