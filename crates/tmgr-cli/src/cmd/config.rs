use crate::cli::ConfigCommand;
use crate::ctx::Ctx;
use crate::out::Output;
use serde_json::json;
use tmgr_core::agent_status::UPLOAD_NOTE;
use tmgr_core::config::{config_path, Config};
use tmgr_core::error::Result;

pub fn run(ctx: &Ctx, cmd: &ConfigCommand) -> Result<Output> {
    match cmd {
        ConfigCommand::List => {
            let list = ctx.cfg.list();
            let human = list
                .iter()
                .map(|(k, v)| format!("{k} = {}", v.as_deref().unwrap_or("")))
                .collect::<Vec<_>>()
                .join("\n");
            let values: serde_json::Map<String, serde_json::Value> = list
                .into_iter()
                .map(|(k, v)| (k.to_string(), json!(v)))
                .collect();
            Ok(Output::new(json!({ "values": values }), human))
        }
        ConfigCommand::Get { key } => {
            let value = ctx.cfg.get(key)?;
            Ok(Output::new(
                json!({"key": key, "value": value}),
                value.clone().unwrap_or_default(),
            ))
        }
        ConfigCommand::Set { key, value } => {
            let mut cfg = Config::load(&ctx.dir)?;
            let stored = cfg.set(key, value)?;
            cfg.save(&ctx.dir)?;
            let mut human = format!("{key} = {stored}");
            if key == "upload_status" && stored == "true" {
                human.push('\n');
                human.push_str(UPLOAD_NOTE);
            }
            Ok(Output::new(json!({"key": key, "value": stored}), human))
        }
        ConfigCommand::Path => {
            let path = config_path(&ctx.dir).display().to_string();
            Ok(Output::new(json!({ "path": path }), path))
        }
    }
}
