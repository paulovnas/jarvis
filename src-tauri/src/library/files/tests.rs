use super::*;

#[test]
fn explorer_lists_one_level_with_folders_first_and_hidden_unicode_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    fs::create_dir(root.join("código fonte")).unwrap();
    fs::write(root.join("código fonte/árvore.ts"), "const valor = 1;\r\n").unwrap();
    fs::write(root.join(".env.example"), "EXAMPLE=demo").unwrap();
    let listing = directory(&root, "").unwrap();
    assert_eq!(listing.entries.len(), 2);
    assert_eq!(listing.entries[0].name, "código fonte");
    assert_eq!(listing.entries[0].kind, EntryKind::Directory);
    assert_eq!(listing.entries[1].name, ".env.example");
    assert!(!listing.truncated);
    let nested = directory(&root, "código fonte").unwrap();
    assert_eq!(nested.entries[0].path, "código fonte/árvore.ts");
    let opened = preview(&root, "código fonte\\árvore.ts").unwrap();
    assert_eq!(opened.path, "código fonte/árvore.ts");
    assert_eq!(opened.content, "const valor = 1;\r\n");
    assert_eq!(
        fs::read_to_string(root.join(&opened.path)).unwrap(),
        opened.content
    );
}

#[test]
fn explorer_rejects_escape_absolute_drive_stream_and_unavailable_paths() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    for path in [
        "../other",
        "nested/../../other",
        "/etc/passwd",
        "C:\\Windows",
        "\\\\server\\share",
        "file.txt:stream",
        "missing",
        "",
    ] {
        assert!(preview(&root, path).is_err(), "accepted {path}");
    }
    assert!(directory(&root, "../other").is_err());
}

#[test]
fn preview_handles_utf8_bom_utf16_and_rejects_binary_or_oversized_content() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    fs::write(root.join("bom.txt"), b"\xef\xbb\xbfhello").unwrap();
    assert_eq!(preview(&root, "bom.txt").unwrap().content, "hello");
    for (name, little) in [("utf16le.txt", true), ("utf16be.txt", false)] {
        let mut bytes = if little {
            vec![0xff, 0xfe]
        } else {
            vec![0xfe, 0xff]
        };
        for word in "Olá\r\n".encode_utf16() {
            bytes.extend(if little {
                word.to_le_bytes()
            } else {
                word.to_be_bytes()
            });
        }
        fs::write(root.join(name), bytes).unwrap();
        assert_eq!(preview(&root, name).unwrap().content, "Olá\r\n");
    }
    fs::write(root.join("binary"), b"binary\0data").unwrap();
    assert_eq!(preview(&root, "binary").unwrap_err().code, "file_encoding");
    let large = fs::File::create(root.join("large")).unwrap();
    large.set_len(MAX_FILE_BYTES + 1).unwrap();
    assert_eq!(preview(&root, "large").unwrap_err().code, "file_too_large");
    fs::write(root.join("empty"), b"").unwrap();
    assert_eq!(preview(&root, "empty").unwrap().content, "");
}

#[test]
fn explorer_does_not_follow_directory_links_outside_the_project() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let outside = temp.path().join("outside");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("private.txt"), "outside").unwrap();
    let root = root.canonicalize().unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
    #[cfg(windows)]
    junction::create(&outside, root.join("link")).unwrap();
    assert!(directory(&root, "link").is_err());
    assert!(preview(&root, "link/private.txt").is_err());
}
