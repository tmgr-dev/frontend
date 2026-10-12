use std::path::PathBuf;

pub struct SandboxSpec {
    pub session_id: String,
    pub argv: Vec<String>,
    pub cwd: PathBuf,
    pub home: PathBuf,
    pub real_home: PathBuf,
    pub runner_dir: PathBuf,
    pub self_exe: PathBuf,
    pub env_pass: Vec<String>,
    pub base_env: Vec<(String, String)>,
}

pub struct Launch {
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: PathBuf,
}

pub fn prepare(spec: &SandboxSpec) -> Result<Launch, String> {
    Ok(Launch {
        argv: spec.argv.clone(),
        env: vec![
            ("HOME".into(), spec.home.display().to_string()),
            ("TERM".into(), "xterm-256color".into()),
            ("TMGR_SESSION_ID".into(), spec.session_id.clone()),
        ],
        cwd: spec.cwd.clone(),
    })
}

pub fn exec_helper(_args: &[String]) -> i32 {
    2
}
