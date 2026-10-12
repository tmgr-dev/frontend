use std::sync::OnceLock;

use rusqlite::types::Value as Sql;

use crate::router::{unrouted, Route};

mod agent_work;
mod catalog;
mod comments;
mod pages;
mod routines;
mod rust_only;
mod tasks;

pub(crate) fn int(v: i64) -> Sql {
    Sql::Integer(v)
}

pub(crate) fn text(v: &str) -> Sql {
    Sql::Text(v.to_string())
}

/// Same order as the TS `createLocalApi` router, since the first matching route wins.
pub(crate) fn table() -> &'static [Route] {
    static TABLE: OnceLock<Vec<Route>> = OnceLock::new();
    TABLE.get_or_init(|| {
        vec![
            Route::new("GET", "tasks/current", tasks::current),
            Route::new("GET", "tasks/runned", tasks::running),
            Route::new("GET", "tasks/indexes", tasks::indexes),
            Route::new("GET", "tasks/status/:status", tasks::by_status),
            Route::new("GET", "tasks", tasks::list),
            Route::new("GET", "tasks/settings", tasks::settings),
            Route::new("GET", "tasks/:id(\\d+)", tasks::show),
            Route::new(
                "GET",
                "workspaces/:wid/assignable-personas",
                tasks::assignable_personas,
            ),
            Route::new("GET", "tasks/:id(\\d+)/comments", comments::list),
            Route::new("GET", "task-relation-types", tasks::relation_types),
            Route::new("GET", "tasks/:id(\\d+)/relations", tasks::relations),
            Route::new("GET", "workspaces/statuses", catalog::statuses),
            Route::new("GET", "workspaces/:wid/statuses", catalog::statuses),
            Route::new("GET", "statuses/:id(\\d+)", catalog::status),
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
            Route::new("GET", "tasks/:id(\\d+)/agent-work", agent_work::for_task),
            Route::new("GET", "workspaces/:wid/members", catalog::members),
            Route::new(
                "GET",
                "workspaces/:wid/feature-toggles",
                catalog::feature_toggles,
            ),
            Route::new("GET", "daily-routines/expand", routines::expand),
            Route::new("GET", "pages/tree", pages::tree),
            Route::new("GET", "pages/trash", unrouted),
            Route::new("GET", "pages/search", unrouted),
            Route::new("GET", "pages/:id", pages::show),
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
