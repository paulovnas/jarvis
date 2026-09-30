//! Shell execution metadata and a deliberately narrow development-service restart policy.
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum Execution {
    #[default]
    Unknown,
    Idle,
    Background,
    Running {
        command: String,
        cwd: PathBuf,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Restart {
    pub command: String,
    pub cwd: PathBuf,
}

impl Execution {
    pub(super) fn active(&self) -> bool {
        !matches!(self, Self::Idle)
    }

    pub(super) fn restart(&self, root: &Path) -> Option<Restart> {
        let Self::Running { command, cwd } = self else {
            return None;
        };
        restart(command, root, cwd)
    }
}

/// No inference from typed input or terminal logs: only explicit shell hooks
/// change the execution state. Frames carry a per-shell token and are stripped
/// before retained output and renderer events.
pub(super) enum Event {
    Start {
        cwd: PathBuf,
        command: String,
        eligible: bool,
    },
    End {
        cwd: PathBuf,
    },
    Idle {
        cwd: PathBuf,
        background: bool,
    },
}

pub(super) struct Parser {
    token: String,
    pending: Vec<u8>,
}

const PREFIX: &[u8] = b"\x1b]777;jarvis;";
const MAX_FRAME: usize = 48 * 1024;

#[derive(Default)]
pub(super) struct Utf8(Vec<u8>);
impl Utf8 {
    pub(super) fn push(&mut self, bytes: &[u8]) -> String {
        self.0.extend_from_slice(bytes);
        let mut text = String::new();
        let mut consumed = 0;
        while consumed < self.0.len() {
            match std::str::from_utf8(&self.0[consumed..]) {
                Ok(rest) => {
                    text.push_str(rest);
                    consumed = self.0.len();
                }
                Err(error) => {
                    let valid = consumed + error.valid_up_to();
                    text.push_str(
                        std::str::from_utf8(&self.0[consumed..valid]).unwrap_or_default(),
                    );
                    consumed = valid;
                    if let Some(invalid) = error.error_len() {
                        text.push('\u{fffd}');
                        consumed += invalid;
                    } else {
                        break;
                    }
                }
            }
        }
        self.0.drain(..consumed);
        text
    }
}

impl Parser {
    pub(super) fn new(token: String) -> Self {
        Self {
            token,
            pending: Vec::new(),
        }
    }

    pub(super) fn push(&mut self, bytes: &[u8]) -> (Vec<u8>, Vec<Event>) {
        self.pending.extend_from_slice(bytes);
        let mut output = Vec::new();
        let mut events = Vec::new();
        let mut consumed = 0;
        while consumed < self.pending.len() {
            let rest = &self.pending[consumed..];
            if rest.starts_with(PREFIX) {
                if let Some(end) = rest.iter().position(|byte| *byte == 7) {
                    if end <= MAX_FRAME {
                        if let Some(event) = self.decode(&rest[PREFIX.len()..end]) {
                            events.push(event);
                        }
                    }
                    // Even invalid private frames never appear in terminal output.
                    consumed += end + 1;
                } else if rest.len() > MAX_FRAME {
                    // Bound fragmented/malicious metadata without exposing a token.
                    consumed += rest.len();
                } else {
                    break;
                }
            } else if PREFIX.starts_with(rest) {
                break;
            } else {
                output.push(self.pending[consumed]);
                consumed += 1;
            }
        }
        self.pending.drain(..consumed);
        (output, events)
    }

    pub(super) fn finish(&mut self) -> Vec<u8> {
        if self.pending.starts_with(PREFIX) || PREFIX.starts_with(&self.pending) {
            self.pending.clear();
        }
        std::mem::take(&mut self.pending)
    }

    fn decode(&self, bytes: &[u8]) -> Option<Event> {
        let frame = std::str::from_utf8(bytes).ok()?;
        let fields: Vec<_> = frame.split(';').collect();
        if fields.first().copied()? != self.token {
            return None;
        }
        let text = |encoded: &str, limit: usize| {
            let value = String::from_utf8(STANDARD.decode(encoded).ok()?).ok()?;
            (value.len() <= limit && !value.contains('\0')).then_some(value)
        };
        match fields.as_slice() {
            [_, "start", cwd, command] => Some(Event::Start {
                cwd: PathBuf::from(text(cwd, 16_000)?),
                command: text(command, 16_000)?,
                eligible: true,
            }),
            [_, "start", cwd, command, eligible] if matches!(*eligible, "0" | "1") => {
                Some(Event::Start {
                    cwd: PathBuf::from(text(cwd, 16_000)?),
                    command: text(command, 16_000)?,
                    eligible: *eligible == "1",
                })
            }
            [_, "end", code, cwd] if code.parse::<i32>().is_ok() => Some(Event::End {
                cwd: PathBuf::from(text(cwd, 16_000)?),
            }),
            [_, "idle", cwd] => Some(Event::Idle {
                cwd: PathBuf::from(text(cwd, 16_000)?),
                background: false,
            }),
            [_, "idle", cwd, count] => Some(Event::Idle {
                cwd: PathBuf::from(text(cwd, 16_000)?),
                background: count.parse::<u32>().ok()? > 0,
            }),
            _ => None,
        }
    }
}

pub(super) fn scoped_directory(root: &Path, cwd: &Path) -> Option<PathBuf> {
    let root = root.canonicalize().ok()?;
    let cwd = cwd.canonicalize().ok()?;
    (cwd.is_dir() && cwd.starts_with(root)).then_some(cwd)
}

pub(super) fn scriptless_shell(program: &Path, arguments: &[String]) -> bool {
    let name = program
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !program.is_absolute() || !native_executable(program) {
        return false;
    }
    match name.as_str() {
        "bash" | "zsh" => arguments.iter().all(|argument| {
            matches!(
                argument.as_str(),
                "-l" | "-i" | "-li" | "-il" | "--login" | "--interactive" | "--noprofile"
            )
        }),
        "pwsh" | "powershell" => {
            arguments.is_empty()
                || arguments
                    == [
                        "-NoLogo",
                        "-NoExit",
                        "-Command",
                        "[Console]::OutputEncoding=[Text.UTF8Encoding]::new()",
                    ]
        }
        _ => false,
    }
}

fn native_executable(program: &Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(program) else {
        return false;
    };
    let mut header = [0_u8; 4];
    if !file.metadata().is_ok_and(|metadata| metadata.is_file())
        || file.read_exact(&mut header).is_err()
    {
        return false;
    }
    header.starts_with(b"MZ")
        || matches!(
            header,
            [0x7f, b'E', b'L', b'F']
                | [0xfe, 0xed, 0xfa, 0xce | 0xcf]
                | [0xce | 0xcf, 0xfa, 0xed, 0xfe]
                | [0xca, 0xfe, 0xba, 0xbe | 0xbf]
                | [0xbe | 0xbf, 0xba, 0xfe, 0xca]
        )
}

fn separators(command: &str) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    let mut chains = 0;
    let mut chars = command.chars().peekable();
    while let Some(character) = chars.next() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' && quote != Some('\'') {
            escaped = true;
            continue;
        }
        if matches!(character, '\'' | '"') {
            match quote {
                None => quote = Some(character),
                Some(current) if current == character => quote = None,
                _ => {}
            }
            continue;
        }
        if quote.is_none() {
            match character {
                '&' if chars.peek() == Some(&'&') => {
                    chars.next();
                    chains += 1;
                }
                '&' | '|' | ';' | '\n' | '\r' => return None,
                _ => {}
            }
        }
    }
    (!escaped && quote.is_none()).then_some(chains)
}

fn development_service(argv: &[String]) -> bool {
    let words = argv.iter().map(String::as_str).collect::<Vec<_>>();
    match words.as_slice() {
        ["npm" | "bun" | "pnpm" | "yarn", "run", "dev" | "start" | "serve", ..]
        | ["npm" | "bun" | "pnpm" | "yarn", "dev" | "start" | "serve", ..]
        | ["next" | "nuxt" | "astro", "dev", ..]
        | ["webpack", "serve", ..]
        | ["ng", "serve", ..]
        | ["php", "artisan", "serve", ..]
        | ["python" | "python3", "-m", "http.server", ..]
        | ["flask", "run", ..]
        | ["rails", "server" | "s", ..]
        | ["bundle", "exec", "rails", "server" | "s", ..]
        | ["dotnet", "watch", "run", ..]
        | ["uvicorn", _, ..] => true,
        ["vite", rest @ ..] => rest
            .first()
            .is_none_or(|value| value.starts_with('-') || matches!(*value, "dev" | "serve")),
        _ => false,
    }
}

pub(super) fn restart(command: &str, root: &Path, cwd: &Path) -> Option<Restart> {
    if command.is_empty() || command.len() > 16_000 || command.contains('\0') {
        return None;
    }
    let chains = separators(command)?;
    let plan = super::super::execution_policy::parse_command(command).ok()?;
    if plan.dynamic || !plan.redirections.is_empty() {
        return None;
    }
    let cwd = scoped_directory(root, cwd)?;
    match plan.invocations.as_slice() {
        [service] if chains == 0 && development_service(&service.argv) => {}
        [change, service] if chains == 1 && development_service(&service.argv) => {
            let [cd, target] = change.argv.as_slice() else {
                return None;
            };
            if cd != "cd" {
                return None;
            }
            let target = Path::new(target);
            scoped_directory(
                root,
                &if target.is_absolute() {
                    target.to_path_buf()
                } else {
                    cwd.join(target)
                },
            )?;
        }
        _ => return None,
    }
    Some(Restart {
        command: command.into(),
        cwd,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_text_keeps_multibyte_characters_across_pty_reads() {
        let value = "ação 👋";
        for split in 0..value.len() {
            let mut decoder = Utf8::default();
            let first = decoder.push(&value.as_bytes()[..split]);
            let second = decoder.push(&value.as_bytes()[split..]);
            assert_eq!(format!("{first}{second}"), value);
        }
    }

    #[test]
    fn launcher_scripts_are_never_considered_restorable_shells() {
        let directory = tempfile::tempdir().unwrap();
        let program = directory
            .path()
            .join(if cfg!(windows) { "bash.exe" } else { "bash" });
        std::fs::write(&program, b"\x7fELF").unwrap();
        assert!(scriptless_shell(&program, &[]));
        assert!(scriptless_shell(&program, &["-i".into()]));
        assert!(!scriptless_shell(
            &program,
            &["-c".into(), "npm run migrate".into()]
        ));
        assert!(!scriptless_shell(
            &program,
            &["--rcfile".into(), "unknown.sh".into(), "-i".into()]
        ));
        std::fs::write(&program, "#!/bin/sh\nnpm run migrate\nexec /bin/bash\n").unwrap();
        assert!(!scriptless_shell(&program, &[]));
    }

    #[test]
    fn metadata_survives_split_frames_and_is_never_rendered() {
        let frame = format!(
            "before\x1b]777;jarvis;secret;start;{};{}\x07after",
            STANDARD.encode("/tmp/ação"),
            STANDARD.encode("npm run dev")
        );
        for split in 0..frame.len() {
            let mut parser = Parser::new("secret".into());
            let (mut first, mut events) = parser.push(&frame.as_bytes()[..split]);
            let (last, tail) = parser.push(&frame.as_bytes()[split..]);
            first.extend(last);
            events.extend(tail);
            assert_eq!(first, b"beforeafter");
            assert!(
                matches!(events.as_slice(), [Event::Start { command, .. }] if command == "npm run dev")
            );
        }
    }

    #[test]
    fn incorrect_tokens_and_partial_private_frames_do_not_escape_or_change_state() {
        let mut parser = Parser::new("secret".into());
        let (output, events) = parser.push(b"\x1b]777;jarvis;other;idle;L3RtcA==\x07visible");
        assert_eq!(output, b"visible");
        assert!(events.is_empty());
        parser.push(b"\x1b]777;jarvis;secret;start;");
        assert!(parser.finish().is_empty());
    }

    #[test]
    fn an_uncertain_shell_line_cannot_authorize_replaying_its_first_command() {
        let mut parser = Parser::new("secret".into());
        let frame = format!(
            "\x1b]777;jarvis;secret;start;{};{};0\x07",
            STANDARD.encode("/tmp"),
            STANDARD.encode("npm run dev")
        );
        let (output, events) = parser.push(frame.as_bytes());
        assert!(output.is_empty());
        assert!(matches!(
            events.as_slice(),
            [Event::Start {
                eligible: false,
                ..
            }]
        ));
    }

    #[test]
    fn only_active_static_development_commands_inside_project_are_restartable() {
        let directory = tempfile::tempdir().unwrap();
        let sub = directory.path().join("front end");
        std::fs::create_dir(&sub).unwrap();
        for command in [
            "npm run dev",
            "bun dev",
            "cd 'front end' && npm run dev",
            "php artisan serve",
            "python3 -m http.server 8000",
        ] {
            assert!(
                restart(command, directory.path(), directory.path()).is_some(),
                "{command}"
            );
        }
        for command in [
            "npm test",
            "npm run build",
            "npm run migration && npm run dev",
            "npm run dev; npm run migrate",
            "npm run dev | tee log",
            "npm run dev > log",
            "cd .. && npm run dev",
            "npm run $(cat script)",
            "npm run dev &",
            "vite build",
        ] {
            assert!(
                restart(command, directory.path(), directory.path()).is_none(),
                "{command}"
            );
        }
        assert!(!Execution::Idle.active());
        assert!(Execution::Unknown.active());
        assert!(Execution::Idle.restart(directory.path()).is_none());
    }
}
