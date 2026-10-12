use std::sync::OnceLock;

use rusqlite::types::Value as Sql;

use crate::router::{unrouted, Route};

mod agent_work;
mod catalog;
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
/// ported yet are `unrouted`: a persona still gets the gate's refusal, anyone else gets no answer.
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
            Route::new(
                "POST",
                "tasks/:id(\\d+)/related-to/:otherId(\\d+)/with/:typeId(\\d+)",
                unrouted,
            ),
            Route::new(
                "DELETE",
                "tasks/:id(\\d+)/related-to/:otherId(\\d+)/with/:typeId(\\d+)",
                unrouted,
            ),
            Route::new("POST", "files/presign-upload", unrouted),
            Route::new("GET", "tasks/:id(\\d+)/files", unrouted),
            Route::new("POST", "tasks/:id(\\d+)/files", unrouted),
            Route::new("GET", "files/:id(\\d+)", unrouted),
            Route::new("GET", "files/:id(\\d+)/signed-url", unrouted),
            Route::new("GET", "files/:id(\\d+)/content", unrouted),
            Route::new("DELETE", "files/:id(\\d+)", unrouted),
            Route::new("GET", "workspaces/:wid/files", unrouted),
            Route::new("GET", "workspaces/statuses", catalog::statuses),
            Route::new("GET", "workspaces/:wid/statuses", catalog::statuses),
            Route::new("POST", "workspaces/:wid/statuses", unrouted),
            Route::new("PUT", "workspaces/:wid/statuses/order", unrouted),
            Route::new("GET", "statuses/:id(\\d+)", catalog::status),
            Route::new("PUT", "statuses/:id(\\d+)", unrouted),
            Route::new("DELETE", "statuses/:id(\\d+)", unrouted),
            Route::new("PUT", "statuses/:id(\\d+)/tasks", unrouted),
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
            Route::new("POST", "project_categories", unrouted),
            Route::new("PUT", "project_categories/:id(\\d+)", unrouted),
            Route::new("DELETE", "project_categories/:id(\\d+)", unrouted),
            Route::new("GET", "plugins/:pid/storage", unrouted),
            Route::new("GET", "plugins/:pid/storage/:key", unrouted),
            Route::new("PUT", "plugins/:pid/storage/:key", unrouted),
            Route::new("DELETE", "plugins/:pid/storage/:key", unrouted),
            Route::new("GET", "plugins/:pid/tasks/:tid(\\d+)/data/:key", unrouted),
            Route::new("PUT", "plugins/:pid/tasks/:tid(\\d+)/data/:key", unrouted),
            Route::new(
                "DELETE",
                "plugins/:pid/tasks/:tid(\\d+)/data/:key",
                unrouted,
            ),
            Route::new("POST", "plugins/:pid/task-data/query", unrouted),
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
            Route::new("PUT", "workspaces/:wid/feature-toggles", unrouted),
            Route::new("GET", "workspaces/:wid/modules", unrouted),
            Route::new("PUT", "workspaces/:wid/modules", unrouted),
            Route::new("PUT", "workspaces/:wid/modules/hidden", unrouted),
            Route::new("GET", "daily-routines/workspace", unrouted),
            Route::new("GET", "daily-routines/tasks/count", unrouted),
            Route::new("GET", "daily-routines/tasks/archived/count", unrouted),
            Route::new("GET", "daily-routines/tasks/completed/count", unrouted),
            Route::new("POST", "daily-routines/tasks", unrouted),
            Route::new("POST", "daily-routines/tasks/recurring", unrouted),
            Route::new("POST", "daily-routines/tasks/quick", unrouted),
            Route::new("GET", "daily-routines/tasks/upcoming", unrouted),
            Route::new("GET", "daily-routines/tasks", unrouted),
            Route::new("GET", "daily-routines/tasks/:id(\\d+)", unrouted),
            Route::new("PUT", "daily-routines/tasks/:id(\\d+)", unrouted),
            Route::new("DELETE", "daily-routines/tasks/:id(\\d+)", unrouted),
            Route::new(
                "POST",
                "daily-routines/tasks/:id(\\d+)/complete",
                routines::complete,
            ),
            Route::new("POST", "daily-routines/tasks/:id(\\d+)/archive", unrouted),
            Route::new(
                "POST",
                "daily-routines/tasks/:id(\\d+)/complete-on",
                routines::complete_on,
            ),
            Route::new("POST", "daily-routines/tasks/:id(\\d+)/convert", unrouted),
            Route::new("GET", "daily-routines/tasks/:id(\\d+)/instances", unrouted),
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
            Route::new(
                "DELETE",
                "daily-routines/tasks/:id(\\d+)/instances/:instanceId(\\d+)",
                unrouted,
            ),
            Route::new(
                "PATCH",
                "daily-routines/tasks/:id(\\d+)/instances/:instanceId",
                unrouted,
            ),
            Route::new("PUT", "daily-routines/tasks/:id(\\d+)/pattern", unrouted),
            Route::new("GET", "daily-routines/tasks/:id(\\d+)/stats", unrouted),
            Route::new("GET", "daily-routines/expand", routines::expand),
            Route::new("GET", "daily-routines/expand/stats", unrouted),
            Route::new("GET", "workspaces/:wid/settings", unrouted),
            Route::new("PUT", "workspaces/:wid/settings", unrouted),
            Route::new("GET", "workspaces/context", unrouted),
            Route::new("GET", "pages", unrouted),
            Route::new("GET", "pages/tree", pages::tree),
            Route::new("GET", "pages/trash", unrouted),
            Route::new("GET", "pages/search", unrouted),
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
            Route::new(
                "POST",
                "pages/:id/versions/:version(\\d+)/restore",
                unrouted,
            ),
            Route::new("GET", "pages/:id/backlinks", pages::backlinks),
            Route::new("GET", "pages/:id/files", unrouted),
            Route::new("POST", "pages/:id/files", unrouted),
            Route::new("POST", "pages/:id/task-from-selection", unrouted),
            Route::new("POST", "pages/:id/follow", unrouted),
            Route::new("DELETE", "pages/:id/follow", unrouted),
            Route::new("GET", "tasks/:id(\\d+)/pages", unrouted),
            Route::new("GET", "plugins/:pid/pages/:id(\\d+)/data/:key", unrouted),
            Route::new("PUT", "plugins/:pid/pages/:id(\\d+)/data/:key", unrouted),
            Route::new("DELETE", "plugins/:pid/pages/:id(\\d+)/data/:key", unrouted),
            Route::new("POST", "plugins/:pid/page-data/query", unrouted),
            Route::new("GET", "graph/related", unrouted),
            Route::new("GET", "graph/path", unrouted),
            Route::new("GET", "graph/hubs", unrouted),
            Route::new("GET", "graph/orphans", unrouted),
            Route::new("GET", "graph/map", unrouted),
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
