//! The files processes hold open, indexed once per pass (SAFE-09).
use crate::path::AbsPath;
use crate::probe::{OpenFile, OpenFiles, Seen, Unseen};
use std::path::MAIN_SEPARATOR;

const SEP: u8 = MAIN_SEPARATOR as u8;

/// One snapshot of open files, sorted by path. "Is anything open at this path or below it" is
/// two binary searches, however many files are open and however large the folder is.
pub(super) struct OpenIndex {
    /// Sorted by `key`.
    held: Vec<Held>,
    unknown: Option<Unseen>,
    fold_case: bool,
}

struct Held {
    /// The path's bytes, in lowercase where the policy compares names without case.
    key: Box<[u8]>,
    file: OpenFile,
}

impl OpenIndex {
    pub(super) fn new(snapshot: Seen<OpenFiles>, fold_case: bool) -> Self {
        let (files, unknown) = match snapshot {
            Seen::Found(open) => {
                let unlisted = open.unlisted.iter().find(|process| process.mine);
                (
                    open.files,
                    unlisted.map(|process| Unseen::Unlisted(process.pid)),
                )
            }
            Seen::Missing => (Vec::new(), Some(Unseen::Failed)),
            Seen::Unknown(why) => (Vec::new(), Some(why)),
        };
        let mut held: Vec<Held> = files
            .into_iter()
            .map(|file| {
                let mut key = file.path.as_os_str().as_encoded_bytes().to_vec();
                if fold_case {
                    key.make_ascii_lowercase();
                }
                Held {
                    key: key.into(),
                    file,
                }
            })
            .collect();
        held.sort_unstable_by(|a, b| a.key.cmp(&b.key));
        Self {
            held,
            unknown,
            fold_case,
        }
    }

    /// Why a path nothing in the snapshot holds still cannot be called free: the snapshot
    /// failed, or a process of this user kept its open files to itself.
    pub(super) fn unknown(&self) -> Option<Unseen> {
        self.unknown
    }

    /// A process that holds `path` itself open.
    pub(super) fn at(&self, path: &AbsPath, scratch: &mut Vec<u8>) -> Option<&OpenFile> {
        scratch.clear();
        scratch.extend_from_slice(path.bytes());
        if self.fold_case {
            scratch.make_ascii_lowercase();
        }
        let at = self.held.binary_search_by(|held| (*held.key).cmp(scratch));
        Some(&self.held[at.ok()?].file)
    }

    /// A process that holds open `path` or anything below it.
    pub(super) fn at_or_below(&self, path: &AbsPath, scratch: &mut Vec<u8>) -> Option<&OpenFile> {
        if let Some(file) = self.at(path, scratch) {
            return Some(file);
        }
        // `/c/app-wal` sorts between `/c/app` and `/c/app/x`, so the search for the folder's
        // contents starts at `/c/app/`, not at `/c/app`.
        if scratch.last() != Some(&SEP) {
            scratch.push(SEP);
        }
        let first = self.held.partition_point(|held| *held.key < **scratch);
        let held = self.held.get(first)?;
        held.key.starts_with(scratch).then_some(&held.file)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::probe::{Pid, Unlisted};

    fn path(text: &str) -> AbsPath {
        AbsPath::parse(text.as_ref()).unwrap()
    }

    fn index(open: &[&str], fold_case: bool) -> OpenIndex {
        let files = open.iter().enumerate().map(|(i, path)| OpenFile {
            pid: Pid(i as u32 + 1),
            path: path.into(),
        });
        let open = OpenFiles {
            files: files.collect(),
            unlisted: Vec::new(),
        };
        OpenIndex::new(Seen::Found(open), fold_case)
    }

    fn holder(index: &OpenIndex, at: &str) -> Option<u32> {
        let found = index.at_or_below(&path(at), &mut Vec::new());
        found.map(|file| file.pid.0)
    }

    #[test]
    fn a_folder_is_held_by_a_file_at_any_depth_below_it() {
        let index = index(&["/c/app-wal", "/c/app/a/b/x.db", "/c/apple/y"], false);
        assert_eq!(holder(&index, "/c/app"), Some(2));
        assert_eq!(holder(&index, "/c/app/a"), Some(2));
        assert_eq!(holder(&index, "/c/app/a/b/x.db"), Some(2));
        assert_eq!(holder(&index, "/c"), Some(1));
        assert_eq!(holder(&index, "/"), Some(1));
        assert_eq!(holder(&index, "/c/app/a/b/x.db-wal"), None);
        assert_eq!(holder(&index, "/c/app/a/c"), None);
        assert_eq!(holder(&index, "/c/ap"), None);
        assert_eq!(holder(&index, "/d"), None);
    }

    #[test]
    fn an_open_file_holds_itself_and_not_a_longer_name() {
        let index = index(&["/c/x.db"], false);
        let at = |name: &str| index.at(&path(name), &mut Vec::new()).is_some();
        assert!(at("/c/x.db"));
        assert!(!at("/c"));
        assert!(!at("/c/x.db-wal"));
    }

    #[test]
    fn case_is_ignored_only_where_the_policy_ignores_it() {
        let open = ["/Users/a/Library/Caches/Vendor/Blob"];
        let alias = "/users/A/library/caches/VENDOR";
        assert_eq!(holder(&index(&open, true), alias), Some(1));
        assert_eq!(holder(&index(&open, false), alias), None);
    }

    #[test]
    fn only_an_unlisted_process_of_this_user_leaves_the_answer_unknown() {
        let with = |unlisted: &[Unlisted]| {
            let open = OpenFiles {
                files: Vec::new(),
                unlisted: unlisted.to_vec(),
            };
            OpenIndex::new(Seen::Found(open), false).unknown()
        };
        let theirs = Unlisted {
            pid: Pid(1),
            mine: false,
        };
        let mine = Unlisted {
            pid: Pid(7),
            mine: true,
        };
        assert_eq!(with(&[]), None);
        assert_eq!(with(&[theirs]), None);
        assert_eq!(with(&[theirs, mine]), Some(Unseen::Unlisted(Pid(7))));
        let failed = OpenIndex::new(Seen::Unknown(Unseen::Denied), false);
        assert_eq!(failed.unknown(), Some(Unseen::Denied));
    }
}
