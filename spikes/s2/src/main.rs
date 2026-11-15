use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use objc2_foundation::{NSFileManager, NSString, NSURL};

#[derive(Clone, Copy, PartialEq)]
enum Method {
    Cli,
    NsFileManager,
    Rename,
    Finder,
}

impl Method {
    const ALL: [Method; 4] = [Method::Cli, Method::NsFileManager, Method::Rename, Method::Finder];

    fn tag(self) -> &'static str {
        match self {
            Method::Cli => "cli",
            Method::NsFileManager => "nsfm",
            Method::Rename => "rename",
            Method::Finder => "finder",
        }
    }
}

#[derive(Clone, Copy)]
enum Item {
    File,
    Folder,
    App,
    Container,
    Collision,
}

impl Item {
    const ALL: [Item; 5] = [Item::File, Item::Folder, Item::App, Item::Container, Item::Collision];

    fn tag(self) -> &'static str {
        match self {
            Item::File => "file",
            Item::Folder => "folder",
            Item::App => "app",
            Item::Container => "container",
            Item::Collision => "collision",
        }
    }
}

struct Outcome {
    ok: bool,
    detail: String,
    landed: Option<PathBuf>,
}

fn trash_dir() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap()).join(".Trash")
}

fn exists(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok()
}

fn move_to_trash(method: Method, src: &Path) -> Outcome {
    match method {
        Method::Cli => {
            let out = Command::new("/usr/bin/trash").arg("-v").arg(src).output().unwrap();
            let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
            let landed = text
                .rsplit_once(" to \"")
                .map(|(_, rest)| PathBuf::from(rest.trim().trim_end_matches('"')));
            Outcome { ok: out.status.success(), detail: text.trim().replace(['\n', '\t'], " "), landed }
        }
        Method::NsFileManager => {
            let fm = NSFileManager::defaultManager();
            let url = NSURL::fileURLWithPath(&NSString::from_str(&src.to_string_lossy()));
            let mut resulting = None;
            match fm.trashItemAtURL_resultingItemURL_error(&url, Some(&mut resulting)) {
                Ok(()) => {
                    let landed = resulting.and_then(|u| u.path()).map(|p| PathBuf::from(p.to_string()));
                    Outcome { ok: true, detail: String::new(), landed }
                }
                Err(e) => Outcome { ok: false, detail: e.localizedDescription().to_string(), landed: None },
            }
        }
        Method::Rename => rename_into_trash(src),
        Method::Finder => {
            let out = Command::new("/usr/bin/osascript")
                .args([
                    "-e",
                    "on run argv",
                    "-e",
                    "tell application \"Finder\" to set r to delete (POSIX file (item 1 of argv) as alias)",
                    "-e",
                    "return POSIX path of (r as alias)",
                    "-e",
                    "end run",
                ])
                .arg(src)
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let landed = (!stdout.is_empty()).then(|| PathBuf::from(stdout.trim_end_matches('/')));
            Outcome {
                ok: out.status.success(),
                detail: String::from_utf8_lossy(&out.stderr).trim().replace(['\n', '\t'], " "),
                landed,
            }
        }
    }
}

fn rename_into_trash(src: &Path) -> Outcome {
    let name = src.file_name().unwrap().to_string_lossy().to_string();
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (name[..i].to_string(), name[i..].to_string()),
        _ => (name.clone(), String::new()),
    };
    let c_src = CString::new(src.as_os_str().as_bytes()).unwrap();
    for n in 1..100 {
        let candidate = if n == 1 { name.clone() } else { format!("{stem} {n}{ext}") };
        let dst = trash_dir().join(&candidate);
        let c_dst = CString::new(dst.as_os_str().as_bytes()).unwrap();
        let rc = unsafe { libc::renameatx_np(libc::AT_FDCWD, c_src.as_ptr(), libc::AT_FDCWD, c_dst.as_ptr(), libc::RENAME_EXCL) };
        if rc == 0 {
            return Outcome { ok: true, detail: String::new(), landed: Some(dst) };
        }
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() != Some(libc::EEXIST) {
            return Outcome { ok: false, detail: err.to_string(), landed: None };
        }
    }
    Outcome { ok: false, detail: "no free name".into(), landed: None }
}

fn write(p: &Path, bytes: usize) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, vec![b'x'; bytes]).unwrap();
}

fn make_item(item: Item, work: &Path, container: &Path, prefix: &str) -> PathBuf {
    match item {
        Item::File => {
            let p = work.join(format!("{prefix}.txt"));
            write(&p, 4096);
            p
        }
        Item::Folder => {
            let p = work.join(prefix);
            for i in 0..3 {
                write(&p.join(format!("f{i}.txt")), 4096);
            }
            p
        }
        Item::App => {
            let p = work.join(format!("{prefix}.app"));
            write(&p.join("Contents/Info.plist"), 256);
            let exe = p.join("Contents/MacOS/fake");
            std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
            std::fs::write(&exe, "#!/bin/sh\nexit 0\n").unwrap();
            p
        }
        Item::Container => {
            let p = container.join(format!("{prefix}.txt"));
            write(&p, 4096);
            p
        }
        Item::Collision => {
            let p = work.join(format!("{prefix}.txt"));
            write(&p, 4096);
            p
        }
    }
}

fn finder_trash_names(prefix: &str) -> Vec<String> {
    let out = Command::new("/usr/bin/osascript")
        .args([
            "-e",
            "on run argv",
            "-e",
            "set AppleScript's text item delimiters to linefeed",
            "-e",
            "tell application \"Finder\" to set n to name of every item of trash whose name starts with (item 1 of argv)",
            "-e",
            "return n as text",
            "-e",
            "end run",
        ])
        .arg(prefix)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).lines().filter(|l| !l.is_empty()).map(str::to_string).collect()
}

fn suite(work: &Path, container: &Path, reps: usize, run_id: &str) {
    println!("rep\tmethod\titem\tok\tlatency_ms\tsource_gone\tlanded_name\tin_dot_trash\tdetail");
    let mut trashed = Vec::new();
    for rep in 1..=reps {
        for method in Method::ALL {
            for item in Item::ALL {
                let prefix = format!("s2-{run_id}-{}-{}-{rep}", method.tag(), item.tag());
                if let Item::Collision = item {
                    let first = make_item(item, work, container, &prefix);
                    let o = move_to_trash(method, &first);
                    if let Some(l) = o.landed.clone().or_else(|| Some(trash_dir().join(first.file_name().unwrap()))) {
                        trashed.push(l);
                    }
                }
                let src = make_item(item, work, container, &prefix);
                let start = Instant::now();
                let o = move_to_trash(method, &src);
                let ms = start.elapsed().as_secs_f64() * 1000.0;
                let landed = o.landed.clone().or_else(|| {
                    let guess = trash_dir().join(src.file_name().unwrap());
                    exists(&guess).then_some(guess)
                });
                let in_dot_trash = landed.as_ref().is_some_and(|l| l.parent() == Some(trash_dir().as_path()) && exists(l));
                let landed_name = landed
                    .as_ref()
                    .map(|l| l.file_name().unwrap().to_string_lossy().to_string())
                    .unwrap_or_else(|| "?".into());
                if let Some(l) = &landed {
                    trashed.push(l.clone());
                }
                println!(
                    "{rep}\t{}\t{}\t{}\t{ms:.1}\t{}\t{landed_name}\t{in_dot_trash}\t{}",
                    method.tag(),
                    item.tag(),
                    o.ok,
                    !exists(&src),
                    o.detail
                );
            }
        }
    }
    let listed = finder_trash_names(&format!("s2-{run_id}-"));
    println!("# finder_listed\t{}", listed.len());
    for name in &listed {
        println!("# finder_name\t{name}");
    }
}

fn cleanup(run_id: &str, scratch: &Path) {
    std::fs::create_dir_all(scratch).unwrap();
    for name in finder_trash_names(&format!("s2-{run_id}-")) {
        let from = trash_dir().join(&name);
        let to = scratch.join(&name);
        let moved = std::fs::rename(&from, &to).is_ok();
        let removed = moved
            && if std::fs::symlink_metadata(&to).map(|m| m.is_dir()).unwrap_or(false) {
                std::fs::remove_dir_all(&to).is_ok()
            } else {
                std::fs::remove_file(&to).is_ok()
            };
        println!("cleanup\t{name}\tmoved_out={moved}\tremoved={removed}");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("suite") => {
            let work = PathBuf::from(&args[2]);
            let container = PathBuf::from(&args[3]);
            let reps: usize = args[4].parse().unwrap();
            let run_id = &args[5];
            suite(&work, &container, reps, run_id);
        }
        Some("cleanup") => cleanup(&args[2], Path::new(&args[3])),
        Some(m) => {
            let method = Method::ALL.into_iter().find(|x| x.tag() == m).expect("method");
            let src = PathBuf::from(&args[2]);
            let start = Instant::now();
            let o = move_to_trash(method, &src);
            println!(
                "{}\t{:.1}\t{}\t{}",
                o.ok,
                start.elapsed().as_secs_f64() * 1000.0,
                o.landed.map(|l| l.display().to_string()).unwrap_or_else(|| "?".into()),
                o.detail
            );
        }
        None => eprintln!("usage: s2trash suite WORK CONTAINER REPS RUN_ID | s2trash <cli|nsfm|rename|finder> PATH"),
    }
}
