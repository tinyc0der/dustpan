use dustpan_sys::BootVolume;
use std::fmt;
use std::sync::Arc;

/// Where the screens read their environment from. A test points it at a fixture home, so a
/// screen never reads the tester's real folders.
#[derive(Clone)]
pub struct Environment(Arc<Lookup>);

type Lookup = dyn Fn(&str) -> Option<String> + Send + Sync;

impl Environment {
    pub fn process() -> Self {
        Self(Arc::new(|name| std::env::var(name).ok()))
    }

    pub fn fixed(pairs: &[(&str, String)]) -> Self {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .collect();
        Self(Arc::new(move |name| {
            pairs
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        }))
    }

    pub fn get(&self, name: &str) -> Option<String> {
        (self.0)(name)
    }
}

impl Default for Environment {
    fn default() -> Self {
        Self::process()
    }
}

impl fmt::Debug for Environment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Environment")
    }
}

#[derive(Clone, Debug, Default)]
pub struct Facts {
    pub volume: Option<BootVolume>,
    pub env: Environment,
}
