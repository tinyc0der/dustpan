use dustpan_core::config;
use dustpan_core::path::{AbsPath, escaped};
use dustpan_core::paths::{Env, Os};
use dustpan_core::whitelist::{self, Whitelist};

pub fn whitelist(os: Os, env: Env, home: &AbsPath) -> Result<Whitelist, String> {
    let Some(path) = config::whitelist_path(os, env) else {
        let set = match os {
            Os::Windows => "APPDATA is not set",
            Os::MacOs | Os::Linux => "neither an absolute XDG_CONFIG_HOME nor HOME is set",
        };
        return Err(format!("cannot tell where the whitelist is: {set}"));
    };
    let file = AbsPath::parse(path.as_os_str()).map_err(|why| {
        let shown = escaped(path.as_os_str());
        format!("cannot use the whitelist path {shown}: {why}")
    })?;
    let read = dustpan_sys::read_config(&file);
    whitelist::load(os, home, &file, read).map_err(|e| e.to_string())
}
