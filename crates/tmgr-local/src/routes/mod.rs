use std::sync::OnceLock;

use rusqlite::types::Value as Sql;

use crate::router::Route;

mod agent_work;
mod catalog;
mod catalog_writes;
mod comments;
mod page_writes;
mod pages;
mod routines;
mod rust_only;
mod task_writes;
mod tasks;

pub(crate) fn int(v: i64) -> Sql {
    Sql::Integer(v)
}

pub(crate) fn text(v: &str) -> Sql {
    Sql::Text(v.to_string())
}

/// Every route of the TS `createLocalApi` router, in its order since the first match wins. Routes not
/// ported yet are `unported`: a persona still gets the gate's refusal, anyone else gets no answer.
pub(crate) fn table() -> &'static [Route] {
    static TABLE: OnceLock<Vec<Route>> = OnceLock::new();
    TABLE.get_or_init(|| {
        vec![
            Route::new("GET", "tasks/current", tasks::current),
            Route::new("GET", "tasks/runned", tasks::running),
            Route::new("GET", "tasks/indexes", tasks::indexes),
            Route::new("GET", "tasks/status/:status", tasks::by_status),
            Route::new("PUT", "tasks/update-orders", task_writes::update_orders),
            Route::new("POST", "tasks", task_writes::create).created(),
            Route::new("GET", "tasks", tasks::list),
            Route::new("GET", "tasks/settings", tasks::settings),
            Route::new("GET", "tasks/:id(\\d+)", tasks::show),
            Route::new("PUT", "tasks/:id(\\d+)", task_writes::put),
            Route::new("PATCH", "tasks/:id(\\d+)", task_writes::patch),
            Route::new(
                "POST",
                "tasks/:id(\\d+)/personas/:uuid",
                task_writes::assign_persona,
            ),
            Route::new(
                "DELETE",
                "tasks/:id(\\d+)/personas/:uuid",
                task_writes::unassign_persona,
            ),
            Route::new(
                "GET",
                "workspaces/:wid/assignable-personas",
                tasks::assignable_personas,
            ),
            Route::new("DELETE", "tasks/:id(\\d+)", task_writes::delete),
            Route::new("POST", "tasks/:id(\\d+)/countdown", tasks::start_timer),
            Route::new(
                "DELETE",
                "tasks/:id(\\d+)/countdown",
                tasks::stop_timer_route,
            ),
            Route::new("PUT", "tasks/:id(\\d+)/settings", task_writes::put_settings),
            Route::new("PUT", "tasks/:id(\\d+)/time", task_writes::set_time),
            Route::new("PUT", "tasks/:id(\\d+)/:status", task_writes::set_status),
            Route::new("GET", "tasks/:id(\\d+)/comments", comments::list),
            Route::new("POST", "tasks/:id(\\d+)/comments", task_writes::add_comment).created(),
            Route::new("PUT", "comments/:id(\\d+)", task_writes::edit_comment),
            Route::new("DELETE", "comments/:id(\\d+)", task_writes::delete_comment),
            Route::new(
                "POST",
                "comments/:id(\\d+)/reactions/toggle",
                task_writes::toggle_reaction,
            ),
            Route::new("GET", "task-relation-types", tasks::relation_types),
            Route::new("GET", "tasks/:id(\\d+)/relations", tasks::relations),
            Route::unported(
                "POST",
                "tasks/:id(\\d+)/related-to/:otherId(\\d+)/with/:typeId(\\d+)",
            ),
            Route::unported(
                "DELETE",
                "tasks/:id(\\d+)/related-to/:otherId(\\d+)/with/:typeId(\\d+)",
            ),
            Route::unported("POST", "files/presign-upload"),
            Route::unported("GET", "tasks/:id(\\d+)/files"),
            Route::unported("POST", "tasks/:id(\\d+)/files"),
            Route::unported("GET", "files/:id(\\d+)"),
            Route::unported("GET", "files/:id(\\d+)/signed-url"),
            Route::unported("GET", "files/:id(\\d+)/content"),
            Route::unported("DELETE", "files/:id(\\d+)"),
            Route::unported("GET", "workspaces/:wid/files"),
            Route::new("GET", "workspaces/statuses", catalog::statuses),
            Route::new("GET", "workspaces/:wid/statuses", catalog::statuses),
            Route::new(
                "POST",
                "workspaces/:wid/statuses",
                catalog_writes::create_status,
            )
            .created(),
            Route::new(
                "PUT",
                "workspaces/:wid/statuses/order",
                catalog_writes::order_statuses,
            ),
            Route::new("GET", "statuses/:id(\\d+)", catalog::status),
            Route::new("PUT", "statuses/:id(\\d+)", catalog_writes::update_status),
            Route::new(
                "DELETE",
                "statuses/:id(\\d+)",
                catalog_writes::delete_status,
            ),
            Route::new(
                "PUT",
                "statuses/:id(\\d+)/tasks",
                catalog_writes::move_tasks,
            ),
            Route::new("GET", "project_categories", catalog::categories),
            Route::new(
                "GET",
                "project_categories/children",
                catalog::root_categories,
            ),
            Route::new(
                "GET",
                "project_categories/children/:id(\\d+)",
                catalog::child_categories,
            ),
            Route::new(
                "GET",
                "project_categories/:id(\\d+)/with/parents",
                catalog::category_with_parents,
            ),
            Route::new("GET", "project_categories/:id(\\d+)", catalog::category),
            Route::new(
                "POST",
                "project_categories",
                catalog_writes::create_category,
            )
            .created(),
            Route::new(
                "PUT",
                "project_categories/:id(\\d+)",
                catalog_writes::update_category,
            ),
            Route::new(
                "DELETE",
                "project_categories/:id(\\d+)",
                catalog_writes::delete_category,
            ),
            Route::unported("GET", "plugins/:pid/storage"),
            Route::unported("GET", "plugins/:pid/storage/:key"),
            Route::unported("PUT", "plugins/:pid/storage/:key"),
            Route::unported("DELETE", "plugins/:pid/storage/:key"),
            Route::unported("GET", "plugins/:pid/tasks/:tid(\\d+)/data/:key"),
            Route::unported("PUT", "plugins/:pid/tasks/:tid(\\d+)/data/:key"),
            Route::unported("DELETE", "plugins/:pid/tasks/:tid(\\d+)/data/:key"),
            Route::unported("POST", "plugins/:pid/task-data/query"),
            Route::new("GET", "tasks/:id(\\d+)/agent-work", agent_work::for_task),
            Route::new("POST", "tasks/:id(\\d+)/agent-work", agent_work::start).created(),
            Route::new("PATCH", "agent-work/:id(\\d+)", agent_work::update),
            Route::new("POST", "agent-work/:id(\\d+)/finish", agent_work::finish),
            Route::new("GET", "workspaces/:wid/members", catalog::members),
            Route::new(
                "GET",
                "workspaces/:wid/feature-toggles",
                catalog::feature_toggles,
            ),
            Route::new(
                "PUT",
                "workspaces/:wid/feature-toggles",
                catalog_writes::put_feature_toggles,
            ),
            Route::unported("GET", "workspaces/:wid/modules"),
            Route::unported("PUT", "workspaces/:wid/modules"),
            Route::unported("PUT", "workspaces/:wid/modules/hidden"),
            Route::unported("GET", "daily-routines/workspace"),
            Route::unported("GET", "daily-routines/tasks/count"),
            Route::unported("GET", "daily-routines/tasks/archived/count"),
            Route::unported("GET", "daily-routines/tasks/completed/count"),
            Route::unported("POST", "daily-routines/tasks"),
            Route::unported("POST", "daily-routines/tasks/recurring"),
            Route::unported("POST", "daily-routines/tasks/quick"),
            Route::unported("GET", "daily-routines/tasks/upcoming"),
            Route::unported("GET", "daily-routines/tasks"),
            Route::unported("GET", "daily-routines/tasks/:id(\\d+)"),
            Route::unported("PUT", "daily-routines/tasks/:id(\\d+)"),
            Route::unported("DELETE", "daily-routines/tasks/:id(\\d+)"),
            Route::new(
                "POST",
                "daily-routines/tasks/:id(\\d+)/complete",
                routines::complete,
            ),
            Route::unported("POST", "daily-routines/tasks/:id(\\d+)/archive"),
            Route::new(
                "POST",
                "daily-routines/tasks/:id(\\d+)/complete-on",
                routines::complete_on,
            ),
            Route::unported("POST", "daily-routines/tasks/:id(\\d+)/convert"),
            Route::unported("GET", "daily-routines/tasks/:id(\\d+)/instances"),
            Route::new(
                "POST",
                "daily-routines/tasks/:id(\\d+)/instances/:instanceId(\\d+)/complete",
                routines::complete_instance,
            ),
            Route::new(
                "POST",
                "daily-routines/tasks/:id(\\d+)/instances/:instanceId(\\d+)/skip",
                routines::skip_instance,
            ),
            Route::unported(
                "DELETE",
                "daily-routines/tasks/:id(\\d+)/instances/:instanceId(\\d+)",
            ),
            Route::unported(
                "PATCH",
                "daily-routines/tasks/:id(\\d+)/instances/:instanceId",
            ),
            Route::unported("PUT", "daily-routines/tasks/:id(\\d+)/pattern"),
            Route::unported("GET", "daily-routines/tasks/:id(\\d+)/stats"),
            Route::new("GET", "daily-routines/expand", routines::expand),
            Route::unported("GET", "daily-routines/expand/stats"),
            Route::unported("GET", "workspaces/:wid/settings"),
            Route::unported("PUT", "workspaces/:wid/settings"),
            Route::unported("GET", "workspaces/context"),
            Route::unported("GET", "pages"),
            Route::new("GET", "pages/tree", pages::tree),
            Route::unported("GET", "pages/trash"),
            Route::unported("GET", "pages/search"),
            Route::new("POST", "pages", page_writes::create).created(),
            Route::new("GET", "pages/:id", pages::show),
            Route::new("PATCH", "pages/:id", page_writes::update),
            Route::new("POST", "pages/:id/append", page_writes::append),
            Route::new(
                "PUT",
                "pages/:id/sections/:sectionId",
                page_writes::set_section,
            ),
            Route::new("POST", "pages/:id/move", page_writes::move_page),
            Route::new("POST", "pages/:id/pin", page_writes::pin),
            Route::new("POST", "pages/:id/unpin", page_writes::unpin),
            Route::new("DELETE", "pages/:id", page_writes::delete),
            Route::new("POST", "pages/:id/restore", page_writes::restore),
            Route::new("GET", "pages/:id/versions", pages::versions),
            Route::new("GET", "pages/:id/versions/:version(\\d+)", pages::version),
            Route::unported("POST", "pages/:id/versions/:version(\\d+)/restore"),
            Route::new("GET", "pages/:id/backlinks", pages::backlinks),
            Route::unported("GET", "pages/:id/files"),
            Route::unported("POST", "pages/:id/files"),
            Route::unported("POST", "pages/:id/task-from-selection"),
            Route::unported("POST", "pages/:id/follow"),
            Route::unported("DELETE", "pages/:id/follow"),
            Route::unported("GET", "tasks/:id(\\d+)/pages"),
            Route::unported("GET", "plugins/:pid/pages/:id(\\d+)/data/:key"),
            Route::unported("PUT", "plugins/:pid/pages/:id(\\d+)/data/:key"),
            Route::unported("DELETE", "plugins/:pid/pages/:id(\\d+)/data/:key"),
            Route::unported("POST", "plugins/:pid/page-data/query"),
            Route::unported("GET", "graph/related"),
            Route::unported("GET", "graph/path"),
            Route::unported("GET", "graph/hubs"),
            Route::unported("GET", "graph/orphans"),
            Route::unported("GET", "graph/map"),
            Route::new("GET", "workspaces", rust_only::workspaces),
            Route::new("GET", "tasks/by-key/:key", rust_only::task_by_key),
            Route::new(
                "GET",
                "workspaces/:wid/time-summary",
                rust_only::time_summary,
            ),
            Route::new(
                "GET",
                "workspaces/:wid/agent-work",
                agent_work::for_workspace,
            ),
        ]
    })
}
