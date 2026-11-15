use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Terminal;

type Term = Terminal<CrosstermBackend<std::io::Stdout>>;

#[derive(Clone, Copy, PartialEq)]
enum Elevation {
    Sudo,
    Unprivileged,
}

struct Config {
    target: PathBuf,
    auth_cmd: Option<String>,
    drain: bool,
}

#[derive(Default)]
struct Report {
    auth_exit: Option<i32>,
    helper_reply: String,
    helper_exit: Option<i32>,
    drained_bytes: usize,
    drained_events: usize,
    stray_keys_after_resume: Vec<String>,
    target_exists_after: bool,
    elapsed_ms: u128,
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--privileged-helper") {
        return helper();
    }
    let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let config = Config {
        target: PathBuf::from(flag("--target").unwrap_or_else(|| "/private/var/tmp/dustpan-s3/root-owned.txt".into())),
        auth_cmd: flag("--auth-cmd"),
        drain: !args.iter().any(|a| a == "--no-drain"),
    };
    let auto = flag("--auto").map(|m| if m == "sudo" { Elevation::Sudo } else { Elevation::Unprivileged });
    let report_path = flag("--report");

    let mut terminal = enter()?;
    let mut status = vec![format!("target: {}", config.target.display())];
    let outcome = match auto {
        Some(elevation) => {
            draw(&mut terminal, &status)?;
            std::thread::sleep(Duration::from_millis(300));
            let mut report = run_action(&mut terminal, elevation, &config)?;
            status.push(format!("helper replied: {}", report.helper_reply));
            draw(&mut terminal, &status)?;
            let until = Instant::now() + Duration::from_millis(700);
            while Instant::now() < until {
                if event::poll(Duration::from_millis(20))? {
                    if let Event::Key(k) = event::read()? {
                        report.stray_keys_after_resume.push(format!("{:?}", k.code));
                    }
                }
            }
            Some(report)
        }
        None => interactive(&mut terminal, &config, &mut status)?,
    };
    leave(&mut terminal)?;
    if let (Some(report), Some(path)) = (outcome, report_path) {
        std::fs::write(path, render_report(&report))?;
    }
    Ok(())
}

fn interactive(terminal: &mut Term, config: &Config, status: &mut Vec<String>) -> std::io::Result<Option<Report>> {
    let mut last = None;
    loop {
        draw(terminal, status)?;
        if let Event::Key(k) = event::read()? {
            if k.kind != KeyEventKind::Press {
                continue;
            }
            let elevation = match k.code {
                KeyCode::Char('a') => Elevation::Sudo,
                KeyCode::Char('u') => Elevation::Unprivileged,
                KeyCode::Char('q') | KeyCode::Esc => return Ok(last),
                other => {
                    status.push(format!("key {other:?}"));
                    continue;
                }
            };
            let report = run_action(terminal, elevation, config)?;
            status.push(format!(
                "auth exit {:?} · helper replied {} · drained {} bytes, {} events · target exists {}",
                report.auth_exit, report.helper_reply, report.drained_bytes, report.drained_events, report.target_exists_after
            ));
            last = Some(report);
        }
    }
}

fn enter() -> std::io::Result<Term> {
    enable_raw_mode()?;
    execute!(std::io::stdout(), EnterAlternateScreen)?;
    Terminal::new(CrosstermBackend::new(std::io::stdout()))
}

fn leave(terminal: &mut Term) -> std::io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()
}

fn draw(terminal: &mut Term, status: &[String]) -> std::io::Result<()> {
    terminal.draw(|f| {
        let [body, footer] = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).areas(f.area());
        let text = status.join("\n");
        f.render_widget(Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(" s3 privilege spike ")), body);
        f.render_widget(Paragraph::new(" a Admin delete (sudo)  u Unprivileged delete  q Quit"), footer);
    })?;
    Ok(())
}

fn run_action(terminal: &mut Term, elevation: Elevation, config: &Config) -> std::io::Result<Report> {
    let start = Instant::now();
    let mut report = Report::default();
    leave(terminal)?;
    println!("\r\n[s3] suspended the TUI to ask for admin rights");

    let auth = match (elevation, &config.auth_cmd) {
        (_, Some(cmd)) => Command::new("/bin/sh").arg("-c").arg(cmd).status()?,
        (Elevation::Sudo, None) => Command::new("/usr/bin/sudo").arg("-v").status()?,
        (Elevation::Unprivileged, None) => Command::new("/usr/bin/true").status()?,
    };
    report.auth_exit = auth.code();

    if auth.success() {
        let exe = std::env::current_exe()?;
        let mut cmd = match elevation {
            Elevation::Sudo => {
                let mut c = Command::new("/usr/bin/sudo");
                c.arg("-n").arg(&exe);
                c
            }
            Elevation::Unprivileged => Command::new(&exe),
        };
        let mut child = cmd.arg("--privileged-helper").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;
        let mut stdin = child.stdin.take().unwrap();
        writeln!(stdin, "DELETE\t{}", config.target.display())?;
        drop(stdin);
        let mut reply = String::new();
        BufReader::new(child.stdout.take().unwrap()).read_line(&mut reply)?;
        report.helper_reply = reply.trim().to_string();
        report.helper_exit = child.wait()?.code();
    } else {
        report.helper_reply = "not started: authentication failed".into();
    }
    report.target_exists_after = std::fs::symlink_metadata(&config.target).is_ok();

    if config.drain {
        report.drained_bytes = flush_tty_input();
    }
    *terminal = enter()?;
    if config.drain {
        while event::poll(Duration::ZERO)? {
            event::read()?;
            report.drained_events += 1;
        }
    }
    terminal.clear()?;
    report.elapsed_ms = start.elapsed().as_millis();
    Ok(report)
}

fn flush_tty_input() -> usize {
    let mut pending: libc::c_int = 0;
    unsafe {
        libc::ioctl(libc::STDIN_FILENO, libc::FIONREAD, &mut pending);
        libc::tcflush(libc::STDIN_FILENO, libc::TCIFLUSH);
    }
    pending.max(0) as usize
}

fn helper() -> std::io::Result<()> {
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let reply = match line.trim_end().split_once('\t') {
        Some(("DELETE", path)) => delete_fixture(Path::new(path)),
        _ => "ERR\tunknown action".to_string(),
    };
    println!("{reply}\tuid={}", unsafe { libc::geteuid() });
    Ok(())
}

fn delete_fixture(path: &Path) -> String {
    let in_fixture_dir = path.parent().and_then(|p| p.file_name()).is_some_and(|n| n == "dustpan-s3");
    if !in_fixture_dir {
        return "ERR\trefused: not in a dustpan-s3 fixture folder".into();
    }
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_file() => match std::fs::remove_file(path) {
            Ok(()) => "OK\tdeleted".into(),
            Err(e) => format!("ERR\t{e}"),
        },
        Ok(_) => "ERR\trefused: not a regular file".into(),
        Err(e) => format!("ERR\t{e}"),
    }
}

fn render_report(r: &Report) -> String {
    format!(
        "auth_exit\t{:?}\nhelper_reply\t{}\nhelper_exit\t{:?}\ntarget_exists_after\t{}\ndrained_bytes\t{}\ndrained_events\t{}\nstray_keys_after_resume\t{}\nstray_keys\t{}\nelapsed_ms\t{}\n",
        r.auth_exit,
        r.helper_reply,
        r.helper_exit,
        r.target_exists_after,
        r.drained_bytes,
        r.drained_events,
        r.stray_keys_after_resume.len(),
        r.stray_keys_after_resume.join(","),
        r.elapsed_ms
    )
}
