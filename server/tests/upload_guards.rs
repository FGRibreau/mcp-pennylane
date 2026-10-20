//! Guards that run before a file leaves the machine.
//! Exercised on real temporary files, with no network.

mod common;

use std::fs;
use std::path::PathBuf;

use mcp_pennylane::error::ClientError;
use mcp_pennylane::upload::{
    check_filename, check_open_file, detect_content_type, load_file, parse_allowed_dirs,
    read_bounded, UploadFile, UploadLimits, MAX_UPLOAD_BYTES, MAX_UPLOAD_DIRS,
};

use common::{fixture, minimal_pdf, TempDir};

async fn load(path: &str, allowed: &[PathBuf]) -> Result<UploadFile, ClientError> {
    load_file("postFileAttachments", path, allowed, UploadLimits::DEFAULT).await
}

async fn load_path(path: &std::path::Path, allowed: &[PathBuf]) -> Result<UploadFile, ClientError> {
    load(&path.display().to_string(), allowed).await
}

fn refusal(outcome: Result<UploadFile, ClientError>) -> String {
    match outcome {
        Err(ClientError::InvalidArgs {
            operation_id,
            message,
        }) => {
            assert_eq!(operation_id, "postFileAttachments");
            message
        }
        Err(other) => panic!("expected InvalidArgs, got {:?}", other),
        Ok(file) => panic!("expected a refusal, but {} was accepted", file.name),
    }
}

fn accepted(outcome: Result<UploadFile, ClientError>) -> UploadFile {
    match outcome {
        Ok(file) => file,
        Err(e) => panic!("expected the file to be accepted, got {:?}", e),
    }
}

#[tokio::test]
async fn relative_path_is_refused() {
    let message = refusal(load("factures/octobre.pdf", &[]).await);
    assert!(message.contains("absolute path required"), "{message}");
}

#[tokio::test]
async fn tilde_path_is_refused_not_expanded() {
    let message = refusal(load("~/Documents/octobre.pdf", &[]).await);
    assert!(message.contains("absolute path required"), "{message}");
    assert!(message.contains("not expanded"), "{message}");
}

#[tokio::test]
async fn path_longer_than_the_limit_is_refused() {
    let long = format!("/{}", "a".repeat(5_000));
    let message = refusal(load(&long, &[]).await);
    assert!(message.contains("longer than 4096 bytes"), "{message}");
}

#[tokio::test]
async fn missing_file_reports_the_system_cause() {
    let dir = TempDir::new("missing");
    let message = refusal(load_path(&dir.path().join("absent.pdf"), &[]).await);
    assert!(message.contains("cannot read"), "{message}");
    assert!(message.contains("absent.pdf"), "{message}");
}

#[tokio::test]
async fn directory_is_not_a_regular_file() {
    let dir = TempDir::new("directory");
    let message = refusal(load_path(dir.path(), &[]).await);
    assert!(message.contains("not a regular file"), "{message}");
}

#[tokio::test]
async fn empty_file_is_refused_with_its_signature() {
    let dir = TempDir::new("empty");
    let path = dir.write("vide.pdf", b"");
    let message = refusal(load_path(&path, &[]).await);
    assert!(message.contains("empty file"), "{message}");
}

#[tokio::test]
async fn text_file_named_pdf_is_refused_with_signature_and_accepted_types() {
    let dir = TempDir::new("text");
    let path = dir.write("facture.pdf", b"just some notes, not a pdf");
    let message = refusal(load_path(&path, &[]).await);
    assert!(message.contains("unrecognised file type"), "{message}");
    assert!(message.contains("6a 75 73 74 20 73 6f 6d"), "{message}");
    assert!(
        message.contains("PDF, PNG, JPEG, TIFF, BMP, GIF"),
        "{message}"
    );
}

#[tokio::test]
async fn private_key_renamed_pdf_is_refused_and_not_echoed() {
    let dir = TempDir::new("key");
    let begin = ["-----BEGIN ", "OPENSSH PRIVATE KEY-----"].concat();
    let end = ["-----END ", "OPENSSH PRIVATE KEY-----"].concat();
    let body = ["b3BlbnNz", "aC1rZXktdjEAAAAA"].concat();
    let contents = format!("{begin}\n{body}\n{end}\n");
    let path = dir.write("releve.pdf", contents.as_bytes());
    let message = refusal(load_path(&path, &[]).await);
    assert!(message.contains("unrecognised file type"), "{message}");
    assert!(!message.contains("OPENSSH"), "{message}");
    assert!(!message.contains("b3BlbnNz"), "{message}");
}

#[tokio::test]
async fn text_starting_with_bm_is_not_a_bitmap() {
    let dir = TempDir::new("bm");
    let path = dir.write("bmw.bmp", b"BMW maintenance invoice, October 2026");
    let message = refusal(load_path(&path, &[]).await);
    assert!(message.contains("unrecognised file type"), "{message}");
}

#[tokio::test]
async fn pdf_is_accepted_with_content_type_name_and_bytes() {
    let dir = TempDir::new("pdf");
    let pdf = minimal_pdf();
    let path = dir.write("Facture 42.pdf", &pdf);
    let file = accepted(load_path(&path, &[]).await);
    assert_eq!(file.content_type, "application/pdf");
    assert_eq!(file.name, "Facture 42.pdf");
    assert_eq!(file.bytes.as_ref(), pdf.as_slice());
    assert_eq!(file.path, fs::canonicalize(&path).expect("canonical"));
}

#[tokio::test]
async fn pdf_after_a_preamble_is_accepted() {
    let dir = TempDir::new("preamble");
    let mut bytes = vec![b'\n'; 200];
    bytes.extend_from_slice(&minimal_pdf());
    let path = dir.write("scan.pdf", &bytes);
    let file = accepted(load_path(&path, &[]).await);
    assert_eq!(file.content_type, "application/pdf");
}

#[tokio::test]
async fn pdf_signature_past_the_first_kilobyte_is_refused() {
    let dir = TempDir::new("late");
    let mut bytes = vec![b'x'; 1_100];
    bytes.extend_from_slice(&minimal_pdf());
    let path = dir.write("tard.pdf", &bytes);
    let message = refusal(load_path(&path, &[]).await);
    assert!(message.contains("unrecognised file type"), "{message}");
}

#[tokio::test]
async fn every_accepted_format_is_recognised_from_its_bytes() {
    let dir = TempDir::new("formats");
    let cases = [
        ("pdf", minimal_pdf(), "application/pdf"),
        ("png", fixture("pixel.png"), "image/png"),
        ("jpg", fixture("pixel.jpg"), "image/jpeg"),
        ("tif", fixture("pixel.tif"), "image/tiff"),
        ("bmp", fixture("pixel.bmp"), "image/bmp"),
        ("gif", fixture("pixel.gif"), "image/gif"),
    ];
    for (extension, bytes, expected) in cases {
        let path = dir.write(&format!("piece.{extension}"), &bytes);
        let file = accepted(load_path(&path, &[]).await);
        assert_eq!(file.content_type, expected, "format .{extension}");
    }
}

#[tokio::test]
async fn extension_does_not_decide_the_type() {
    let dir = TempDir::new("extension");
    let path = dir.write("photo.pdf", &fixture("pixel.png"));
    let file = accepted(load_path(&path, &[]).await);
    assert_eq!(file.content_type, "image/png");
}

#[test]
fn big_endian_tiff_and_gif89a_are_recognised() {
    assert_eq!(detect_content_type(b"MM\0*\0\0\0\x08"), Some("image/tiff"));
    assert_eq!(detect_content_type(b"GIF89a\x01\0"), Some("image/gif"));
}

#[tokio::test]
async fn file_above_the_limit_is_refused_without_being_read() {
    let dir = TempDir::new("huge");
    let path = dir.path().join("enorme.pdf");
    let file = fs::File::create(&path).expect("create");
    file.set_len(MAX_UPLOAD_BYTES + 1).expect("sparse file");
    drop(file);
    let message = refusal(load_path(&path, &[]).await);
    assert!(
        message.contains(&format!("file is {} bytes", MAX_UPLOAD_BYTES + 1)),
        "{message}"
    );
    assert!(
        message.contains(&format!("the limit is {} bytes", MAX_UPLOAD_BYTES)),
        "{message}"
    );
}

#[tokio::test]
async fn file_exactly_at_the_limit_is_accepted_and_one_byte_more_is_not() {
    let dir = TempDir::new("limit");
    let mut bytes = minimal_pdf();
    let limits = UploadLimits {
        max_bytes: bytes.len() as u64,
        ..UploadLimits::DEFAULT
    };
    let path = dir.write("juste.pdf", &bytes);
    let display = path.display().to_string();
    accepted(load_file("postFileAttachments", &display, &[], limits).await);

    bytes.push(b'\n');
    let path = dir.write("trop.pdf", &bytes);
    let display = path.display().to_string();
    let message = refusal(load_file("postFileAttachments", &display, &[], limits).await);
    assert!(message.contains("the limit is"), "{message}");
}

#[tokio::test]
async fn file_that_grows_after_the_size_check_is_refused_by_the_bounded_read() {
    let dir = TempDir::new("growing");
    let pdf = minimal_pdf();
    let limit = pdf.len() as u64;
    let path = dir.write("grossit.pdf", &pdf);

    let file = tokio::fs::File::open(&path).await.expect("open");
    let length = check_open_file(&file, limit)
        .await
        .expect("fits when checked");

    let mut writer = fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .expect("reopen for append");
    std::io::Write::write_all(&mut writer, &vec![b'x'; 64 * 1024]).expect("append");
    drop(writer);

    let message = read_bounded(file, length, limit)
        .await
        .expect_err("a file that outgrew the limit must not be returned")
        .to_string();
    assert!(message.contains("grew while being read"), "{message}");
}

#[tokio::test]
async fn bounded_read_returns_an_unchanged_file_whole() {
    let dir = TempDir::new("stable");
    let pdf = minimal_pdf();
    let path = dir.write("stable.pdf", &pdf);
    let file = tokio::fs::File::open(&path).await.expect("open");
    let length = check_open_file(&file, pdf.len() as u64)
        .await
        .expect("fits");
    let read = read_bounded(file, length, pdf.len() as u64)
        .await
        .expect("read");
    assert_eq!(read, pdf);
}

#[tokio::test]
async fn path_outside_the_allowed_directories_is_refused() {
    let dir = TempDir::new("outside");
    let allowed = dir.subdir("factures");
    let elsewhere = dir.subdir("contrats");
    let path = dir.write("contrats/contrat.pdf", &minimal_pdf());
    assert!(path.starts_with(&elsewhere));

    let allowed = parse_allowed_dirs(&allowed.display().to_string()).expect("allowed dirs");
    let message = refusal(load_path(&path, &allowed).await);
    assert!(
        message.contains("outside the directories allowed"),
        "{message}"
    );
    assert!(message.contains("PENNYLANE_UPLOAD_DIRS"), "{message}");
    assert!(
        message.contains(&allowed[0].display().to_string()),
        "{message}"
    );
}

#[tokio::test]
async fn path_inside_an_allowed_directory_is_accepted() {
    let dir = TempDir::new("inside");
    dir.subdir("factures");
    let path = dir.write("factures/octobre.pdf", &minimal_pdf());
    let allowed = parse_allowed_dirs(&dir.path().join("factures").display().to_string())
        .expect("allowed dirs");
    accepted(load_path(&path, &allowed).await);
}

#[tokio::test]
async fn without_a_list_every_directory_is_allowed() {
    let dir = TempDir::new("open");
    let path = dir.write("libre.pdf", &minimal_pdf());
    accepted(load_path(&path, &[]).await);
}

#[tokio::test]
async fn directory_sharing_only_a_name_prefix_is_not_inside_the_allowed_one() {
    let dir = TempDir::new("prefix");
    dir.subdir("a");
    dir.subdir("ab");
    let path = dir.write("ab/piece.pdf", &minimal_pdf());
    let allowed =
        parse_allowed_dirs(&dir.path().join("a").display().to_string()).expect("allowed dirs");
    let message = refusal(load_path(&path, &allowed).await);
    assert!(
        message.contains("outside the directories allowed"),
        "{message}"
    );
}

#[test]
fn allowed_directories_are_resolved_once_at_startup() {
    let dir = TempDir::new("resolve");
    let sub = dir.subdir("factures");
    let listed = sub.join("..").join("factures");
    let resolved = parse_allowed_dirs(&listed.display().to_string()).expect("allowed dirs");
    assert_eq!(resolved, vec![fs::canonicalize(&sub).expect("canonical")]);
}

#[test]
fn allowed_directories_reject_what_cannot_confine_uploads() {
    let dir = TempDir::new("badlist");
    let file = dir.write("fichier.txt", b"x");

    let relative = parse_allowed_dirs("factures").expect_err("relative");
    assert!(relative.to_string().contains("not an absolute path"));

    let absent =
        parse_allowed_dirs(&dir.path().join("absent").display().to_string()).expect_err("absent");
    assert!(absent.to_string().contains("cannot resolve"));

    let not_a_directory = parse_allowed_dirs(&file.display().to_string()).expect_err("file");
    assert!(not_a_directory.to_string().contains("not a directory"));

    let empty = parse_allowed_dirs("").expect_err("empty");
    assert!(empty.to_string().contains("empty entry"));

    #[cfg(unix)]
    {
        let joined = format!("{}:", dir.path().display());
        let trailing = parse_allowed_dirs(&joined).expect_err("empty entry");
        assert!(trailing.to_string().contains("empty entry"));
    }
}

#[test]
fn the_list_of_allowed_directories_is_capped() {
    let dir = TempDir::new("cap");
    let entries: Vec<PathBuf> = (0..=MAX_UPLOAD_DIRS)
        .map(|i| dir.subdir(&format!("d{i}")))
        .collect();

    let at_cap = std::env::join_paths(&entries[..MAX_UPLOAD_DIRS]).expect("join");
    let parsed = parse_allowed_dirs(&at_cap.to_string_lossy()).expect("at the cap");
    assert_eq!(parsed.len(), MAX_UPLOAD_DIRS);

    let over_cap = std::env::join_paths(&entries).expect("join");
    let error = parse_allowed_dirs(&over_cap.to_string_lossy()).expect_err("over the cap");
    assert!(error.to_string().contains("at most 32"), "{error}");
}

#[test]
fn filename_limit_counts_characters_not_bytes() {
    assert!(check_filename(&"a".repeat(255)).is_ok());
    assert!(check_filename(&"é".repeat(255)).is_ok());
    let message = check_filename(&"a".repeat(256))
        .expect_err("256 characters")
        .to_string();
    assert!(message.contains("256 characters"), "{message}");
    assert!(message.contains("limit is 255"), "{message}");
}

#[test]
fn filename_must_be_a_single_printable_line() {
    assert!(check_filename("").is_err());
    assert!(check_filename("facture\r\nContent-Type: text/html").is_err());
    assert!(check_filename("facture\tavril.pdf").is_err());
    assert!(check_filename("Relevé de juillet-août (copie).pdf").is_ok());
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::unix::fs::symlink;
    use std::process::Command;
    use std::time::Duration;

    #[tokio::test]
    async fn symlink_to_a_pdf_is_followed_and_keeps_its_own_name() {
        let dir = TempDir::new("link-pdf");
        let target = dir.write("original.pdf", &minimal_pdf());
        let link = dir.path().join("Facture mars.pdf");
        symlink(&target, &link).expect("symlink");
        let file = accepted(load_path(&link, &[]).await);
        assert_eq!(file.content_type, "application/pdf");
        assert_eq!(file.name, "Facture mars.pdf");
        assert_eq!(file.path, fs::canonicalize(&target).expect("canonical"));
    }

    #[tokio::test]
    async fn symlink_to_a_text_file_is_refused_on_the_real_bytes() {
        let dir = TempDir::new("link-text");
        let target = dir.write("notes.txt", b"private notes");
        let link = dir.path().join("facture.pdf");
        symlink(&target, &link).expect("symlink");
        let message = refusal(load_path(&link, &[]).await);
        assert!(message.contains("unrecognised file type"), "{message}");
    }

    #[tokio::test]
    async fn symlink_leaving_the_allowed_directory_is_judged_on_its_target() {
        let dir = TempDir::new("link-out");
        let allowed = dir.subdir("factures");
        let secret = dir.write("contrat.pdf", &minimal_pdf());
        let link = allowed.join("innocent.pdf");
        symlink(&secret, &link).expect("symlink");
        let allowed = parse_allowed_dirs(&allowed.display().to_string()).expect("allowed dirs");
        let message = refusal(load_path(&link, &allowed).await);
        assert!(
            message.contains("outside the directories allowed"),
            "{message}"
        );
    }

    #[tokio::test]
    async fn device_file_is_not_a_regular_file() {
        let message = refusal(load("/dev/null", &[]).await);
        assert!(message.contains("not a regular file"), "{message}");
    }

    #[tokio::test]
    async fn bounded_read_stops_a_source_that_never_ends() {
        let endless = tokio::fs::File::open("/dev/zero").await.expect("open");
        let outcome =
            tokio::time::timeout(Duration::from_secs(10), read_bounded(endless, 0, 4_096))
                .await
                .expect("the read must stop at the limit");
        let message = outcome
            .expect_err("an endless source exceeds any limit")
            .to_string();
        assert!(message.contains("grew while being read"), "{message}");
    }

    #[tokio::test]
    async fn fifo_is_refused_without_waiting_for_a_writer() {
        let dir = TempDir::new("fifo");
        let fifo = dir.path().join("tube.pdf");
        let status = Command::new("mkfifo").arg(&fifo).status().expect("mkfifo");
        assert!(status.success(), "mkfifo failed");
        let outcome = tokio::time::timeout(Duration::from_secs(10), load_path(&fifo, &[]))
            .await
            .expect("opening a FIFO must not block");
        let message = refusal(outcome);
        assert!(message.contains("not a regular file"), "{message}");
    }

    #[test]
    fn allowed_directory_given_through_a_symlink_is_stored_resolved() {
        let dir = TempDir::new("link-dir");
        let real = dir.subdir("reel");
        let alias = dir.path().join("alias");
        symlink(&real, &alias).expect("symlink");
        let resolved = parse_allowed_dirs(&alias.display().to_string()).expect("allowed dirs");
        assert_eq!(resolved, vec![fs::canonicalize(&real).expect("canonical")]);
    }
}
