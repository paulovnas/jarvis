use super::{AgentError, ToolCall};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Read,
    path::{Component, Path, PathBuf},
};

const MAX_INSTRUCTION_FILE: u64 = 64 * 1024;
const MAX_INSTRUCTIONS_PER_TURN: usize = 256 * 1024;

struct LoadedInstruction {
    relative: String,
    content: String,
}

pub(super) struct Resolver {
    root: PathBuf,
    loaded: BTreeMap<PathBuf, [u8; 32]>,
    instructions: Vec<LoadedInstruction>,
    bytes: usize,
}

impl Resolver {
    pub(super) fn new(root: &Path) -> Result<Self, AgentError> {
        let root = fs::canonicalize(root).map_err(|_| invalid_path())?;
        if !root.is_dir() {
            return Err(invalid_path());
        }
        let mut loaded = BTreeMap::new();
        let root_instructions = root.join("AGENTS.md");
        if let Ok((_, hash)) = read_instruction(&root_instructions) {
            loaded.insert(root_instructions, hash);
        }
        Ok(Self {
            root,
            loaded,
            instructions: Vec::new(),
            bytes: 0,
        })
    }

    pub(super) fn discover(&mut self, tool: &ToolCall) -> Result<bool, AgentError> {
        let mut directories = Vec::new();
        match tool.name.as_str() {
            "list" | "search" => {
                if let Some(path) = tool.args["path"].as_str() {
                    directories.push(self.safe_path(path)?);
                }
            }
            "read" | "write" | "edit" | "lsp_definition" | "lsp_references" | "lsp_symbols"
            | "lsp_diagnostics" => {
                if let Some(path) = tool.args["path"].as_str() {
                    let target = self.safe_path(path)?;
                    directories.push(target.parent().unwrap_or(&self.root).to_path_buf());
                }
            }
            "apply_patch" => {
                for path in super::patch::target_paths(&tool.args)? {
                    let target = self.safe_path(&path)?;
                    directories.push(target.parent().unwrap_or(&self.root).to_path_buf());
                }
            }
            _ => return Ok(false),
        }
        let mut added = false;
        for directory in directories {
            for path in self.candidates(&directory)? {
                if self.loaded.contains_key(&path) {
                    continue;
                }
                let (content, hash) = read_instruction(&path)?;
                if self.bytes.saturating_add(content.len()) > MAX_INSTRUCTIONS_PER_TURN {
                    return Err(AgentError::new(
                        "project_instructions_limit",
                        "As instruções AGENTS.md deste escopo excedem 256 KiB. Reduza ou divida as regras antes de continuar.",
                    ));
                }
                let relative = path
                    .strip_prefix(&self.root)
                    .map_err(|_| invalid_path())?
                    .to_string_lossy()
                    .replace('\\', "/");
                self.bytes += content.len();
                self.loaded.insert(path, hash);
                self.instructions
                    .push(LoadedInstruction { relative, content });
                added = true;
            }
        }
        Ok(added)
    }

    pub(super) fn append_prompt(&self, target: &mut String) {
        for instruction in &self.instructions {
            target.push_str(&format!(
                "\nScoped project instructions from {}. Apply them to every file under that directory; deeper AGENTS.md instructions take precedence for their subtree:\n{}\n",
                instruction.relative, instruction.content
            ));
        }
    }

    fn safe_path(&self, value: &str) -> Result<PathBuf, AgentError> {
        let supplied = Path::new(value);
        let relative = if supplied.is_absolute() {
            supplied
                .strip_prefix(&self.root)
                .map_err(|_| invalid_path())?
        } else {
            supplied
        };
        let mut path = self.root.clone();
        for component in relative.components() {
            match component {
                Component::CurDir => continue,
                Component::Normal(part) => path.push(part),
                _ => return Err(invalid_path()),
            }
            match fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.file_type().is_symlink() => return Err(invalid_path()),
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(invalid_path()),
            }
        }
        Ok(path)
    }

    fn candidates(&self, directory: &Path) -> Result<Vec<PathBuf>, AgentError> {
        let relative = directory
            .strip_prefix(&self.root)
            .map_err(|_| invalid_path())?;
        let mut current = self.root.clone();
        let mut candidates = Vec::new();
        for component in relative.components() {
            let Component::Normal(part) = component else {
                return Err(invalid_path());
            };
            current.push(part);
            let candidate = current.join("AGENTS.md");
            match fs::symlink_metadata(&candidate) {
                Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                    candidates.push(candidate)
                }
                Ok(_) => return Err(invalid_instruction()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(invalid_instruction()),
            }
        }
        Ok(candidates)
    }
}

fn read_instruction(path: &Path) -> Result<(String, [u8; 32]), AgentError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| invalid_instruction())?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_INSTRUCTION_FILE
    {
        return Err(invalid_instruction());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path).map_err(|_| invalid_instruction())?;
    let opened = file.metadata().map_err(|_| invalid_instruction())?;
    if !opened.is_file() || opened.len() != metadata.len() {
        return Err(invalid_instruction());
    }
    let mut bytes = Vec::with_capacity(opened.len() as usize);
    file.take(MAX_INSTRUCTION_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid_instruction())?;
    if bytes.len() as u64 > MAX_INSTRUCTION_FILE || bytes.contains(&0) {
        return Err(invalid_instruction());
    }
    let content = String::from_utf8(bytes).map_err(|_| invalid_instruction())?;
    let hash = Sha256::digest(content.as_bytes()).into();
    Ok((content, hash))
}

fn invalid_path() -> AgentError {
    AgentError::new(
        "project_instructions_path",
        "Não foi possível determinar com segurança as instruções AGENTS.md deste caminho.",
    )
}

fn invalid_instruction() -> AgentError {
    AgentError::new(
        "project_instructions_invalid",
        "Um AGENTS.md deste escopo não é um arquivo UTF-8 comum com até 64 KiB.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tests::Fixture;
    use serde_json::json;

    fn tool(name: &str, path: &str) -> ToolCall {
        ToolCall {
            id: "call".into(),
            name: name.into(),
            args: json!({"path":path}),
            status: "pending".into(),
            output: String::new(),
            duration_ms: 0,
        }
    }

    #[test]
    fn nested_instructions_load_once_without_leaking_from_siblings() {
        let fixture = Fixture::new();
        fs::write(fixture.root.join("AGENTS.md"), "root").unwrap();
        fs::create_dir_all(fixture.root.join("packages/a/src")).unwrap();
        fs::create_dir_all(fixture.root.join("packages/b/src")).unwrap();
        fs::write(fixture.root.join("packages/a/AGENTS.md"), "only a").unwrap();
        fs::write(fixture.root.join("packages/b/AGENTS.md"), "only b").unwrap();
        let mut resolver = Resolver::new(&fixture.root).unwrap();
        assert!(resolver
            .discover(&tool("read", "packages/a/src/app.ts"))
            .unwrap());
        assert!(!resolver
            .discover(&tool("edit", "packages/a/src/app.ts"))
            .unwrap());
        let mut prompt = String::new();
        resolver.append_prompt(&mut prompt);
        assert!(prompt.contains("packages/a/AGENTS.md"));
        assert!(prompt.contains("only a"));
        assert!(!prompt.contains("only b"));
        assert!(!prompt.contains("\nroot\n"));
    }

    #[test]
    fn a_new_turn_reads_changed_scoped_instructions() {
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.root.join("crate/src")).unwrap();
        let path = fixture.root.join("crate/AGENTS.md");
        fs::write(&path, "first").unwrap();
        let mut first = Resolver::new(&fixture.root).unwrap();
        first.discover(&tool("read", "crate/src/lib.rs")).unwrap();
        fs::write(path, "second").unwrap();
        let mut second = Resolver::new(&fixture.root).unwrap();
        second.discover(&tool("read", "crate/src/lib.rs")).unwrap();
        let mut prompt = String::new();
        second.append_prompt(&mut prompt);
        assert!(prompt.contains("second"));
        assert!(!prompt.contains("first"));
    }

    #[test]
    fn a_multi_file_patch_discovers_every_scoped_instruction_before_mutation() {
        let fixture = Fixture::new();
        for package in ["a", "b"] {
            fs::create_dir_all(fixture.root.join(format!("packages/{package}/src"))).unwrap();
            fs::write(
                fixture.root.join(format!("packages/{package}/AGENTS.md")),
                format!("rules {package}"),
            )
            .unwrap();
        }
        let patch = ToolCall {
            id: "patch".into(),
            name: "apply_patch".into(),
            args: json!({"patchText":"*** Begin Patch\n*** Add File: packages/a/src/a.ts\n+a\n*** Add File: packages/b/src/b.ts\n+b\n*** End Patch"}),
            status: "pending".into(),
            output: String::new(),
            duration_ms: 0,
        };
        let mut resolver = Resolver::new(&fixture.root).unwrap();
        assert!(resolver.discover(&patch).unwrap());
        let mut prompt = String::new();
        resolver.append_prompt(&mut prompt);
        assert!(prompt.contains("packages/a/AGENTS.md"));
        assert!(prompt.contains("packages/b/AGENTS.md"));
    }

    #[test]
    fn oversized_and_external_instructions_fail_closed() {
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.root.join("large/src")).unwrap();
        fs::write(
            fixture.root.join("large/AGENTS.md"),
            vec![b'x'; MAX_INSTRUCTION_FILE as usize + 1],
        )
        .unwrap();
        let mut resolver = Resolver::new(&fixture.root).unwrap();
        assert_eq!(
            resolver
                .discover(&tool("read", "large/src/lib.rs"))
                .unwrap_err()
                .code,
            "project_instructions_invalid"
        );
        assert_eq!(
            resolver
                .discover(&tool("read", "../outside.rs"))
                .unwrap_err()
                .code,
            "project_instructions_path"
        );
    }
}
