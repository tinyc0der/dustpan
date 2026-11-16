use crate::error::{Error, Result};
use serde::Deserialize;
use std::collections::BTreeSet;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnownDifference {
    pub command: String,
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct KnownDifferences {
    pub known: Vec<KnownDifference>,
}

impl KnownDifferences {
    pub fn parse(text: &str) -> Result<KnownDifferences> {
        toml::from_str(text).map_err(|e| Error::Manifest(format!("known-differences: {e}")))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Diff {
    pub only_mole: Vec<String>,
    pub only_dustpan: Vec<String>,
    pub both: Vec<String>,
}

pub fn diff<'a>(
    mole: impl IntoIterator<Item = &'a str>,
    dustpan: impl IntoIterator<Item = &'a str>,
) -> Diff {
    let mole: BTreeSet<&str> = mole.into_iter().collect();
    let dustpan: BTreeSet<&str> = dustpan.into_iter().collect();
    let own = |set: Vec<&&str>| set.into_iter().map(|s| s.to_string()).collect();
    Diff {
        only_mole: own(mole.difference(&dustpan).collect()),
        only_dustpan: own(dustpan.difference(&mole).collect()),
        both: own(mole.intersection(&dustpan).collect()),
    }
}

impl Diff {
    pub fn render(&self, dustpan_note: &str, known: &KnownDifferences, command: &str) -> String {
        let mut out = String::new();
        section(&mut out, "only Mole found", &self.only_mole, "");
        section(
            &mut out,
            "only dustpan found",
            &self.only_dustpan,
            dustpan_note,
        );
        section(&mut out, "both found", &self.both, "");
        let mine: Vec<&KnownDifference> = known
            .known
            .iter()
            .filter(|k| k.command == command)
            .collect();
        out.push_str(&format!(
            "== known differences, Mole omits these on purpose ({}) ==\n",
            mine.len()
        ));
        for k in mine {
            out.push_str(&format!("  {}: {}\n", k.path, k.reason));
        }
        out
    }
}

fn section(out: &mut String, title: &str, rows: &[String], note: &str) {
    out.push_str(&format!("== {title} ({}) ==\n", rows.len()));
    if rows.is_empty() && !note.is_empty() {
        out.push_str(&format!("  ({note})\n"));
    }
    for r in rows {
        out.push_str(&format!("  {r}\n"));
    }
}
