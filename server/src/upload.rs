//! Local guards and preparation for `multipart/form-data` operations.
//! A model picks the path, maybe from untrusted content.
//! Every check runs here, before any byte leaves.

use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use bytes::Bytes;
use serde_json::{json, Map, Value};
use tokio::fs::File;
use tokio::io::AsyncReadExt;

use crate::client::{FormPart, MultipartBody};
use crate::error::ClientError;
use crate::generated::GeneratedTool;

/// Ceiling Pennylane documents for an uploaded file.
pub const MAX_UPLOAD_BYTES: u64 = 100_000_000;
pub const MAX_UPLOAD_DIRS: usize = 32;
const MAX_FILENAME_CHARS: usize = 255;
const MAX_PATH_BYTES: usize = 4_096;
const MAX_FIELD_BYTES: usize = 1_048_576;
const PDF_SIGNATURE_WINDOW: usize = 1_024;
const SIGNATURE_PREVIEW_BYTES: usize = 8;
const ACCEPTED_TYPES: &str = "PDF, PNG, JPEG, TIFF, BMP, GIF";

/// Why a file or field was refused before sending.
#[derive(thiserror::Error, Debug)]
pub enum UploadError {
    #[error("`file` is longer than {limit} bytes, the longest accepted path")]
    PathTooLong { limit: usize },
    #[error("absolute path required for `file`: `{}` is relative (`~` is not expanded)", .path.display())]
    NotAbsolute { path: PathBuf },
    #[error("cannot read `{}`: {source}", .path.display())]
    Unreadable {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("`{}` is outside the directories allowed for uploads by PENNYLANE_UPLOAD_DIRS ({allowed})", .path.display())]
    OutsideAllowedDirs { path: PathBuf, allowed: String },
    #[error("`{}` is not a regular file", .path.display())]
    NotRegularFile { path: PathBuf },
    #[error("cannot open `{}`: {source}", .path.display())]
    CannotOpen {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot stat the open file: {0}")]
    CannotStat(#[source] std::io::Error),
    #[error("not a regular file")]
    OpenedNotRegularFile,
    #[error("file is {size} bytes, the limit is {limit} bytes")]
    TooLarge { size: u64, limit: u64 },
    #[error("cannot read the file: {0}")]
    CannotRead(#[source] std::io::Error),
    #[error("file is larger than the {limit} bytes limit (it grew while being read)")]
    GrewWhileReading { limit: u64 },
    #[error(
        "unrecognised file type (first bytes: {}); accepted types are {}, checked on the content and not on the extension",
        signature_preview(.first_bytes),
        ACCEPTED_TYPES
    )]
    UnrecognisedType { first_bytes: Vec<u8> },
    #[error("filename must not be empty")]
    EmptyFilename,
    #[error("filename is {length} characters, the limit is {limit}")]
    FilenameTooLong { length: usize, limit: usize },
    #[error("filename must not contain control characters")]
    FilenameControlCharacters,
    #[error("`{name}` is {size} bytes, the limit is {limit} bytes")]
    FieldTooLarge {
        name: String,
        size: usize,
        limit: usize,
    },
    #[error("`{name}` must be one of {}", .allowed.join(", "))]
    FieldNotAllowed { name: String, allowed: Vec<String> },
}

/// A validated file, ready to attach to a form.
pub struct UploadFile {
    /// Real path that was read, symbolic links resolved.
    pub path: PathBuf,
    /// Name of the file as the caller wrote the path.
    pub name: String,
    pub content_type: &'static str,
    pub bytes: Bytes,
}

impl fmt::Debug for UploadFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UploadFile")
            .field("path", &self.path)
            .field("name", &self.name)
            .field("content_type", &self.content_type)
            .field("size_bytes", &self.bytes.len())
            .finish()
    }
}

#[derive(Debug)]
pub struct PreparedUpload {
    pub body: MultipartBody,
    /// Real path that was read, for the audit trail.
    pub path: PathBuf,
}

/// Parse `PENNYLANE_UPLOAD_DIRS`, a platform path list.
/// Entries are resolved once at startup.
/// A symbolic link therefore cannot widen the area later.
pub fn parse_allowed_dirs(raw: &str) -> Result<Vec<PathBuf>> {
    let entries: Vec<PathBuf> = std::env::split_paths(raw).collect();
    if entries.is_empty() || entries.iter().any(|dir| dir.as_os_str().is_empty()) {
        bail!("the list has an empty entry; unset the variable to allow any directory");
    }
    if entries.len() > MAX_UPLOAD_DIRS {
        bail!(
            "{} directories listed, at most {} are accepted",
            entries.len(),
            MAX_UPLOAD_DIRS
        );
    }
    entries
        .iter()
        .map(|dir| {
            if !dir.is_absolute() {
                bail!("`{}` is not an absolute path", dir.display());
            }
            let real = std::fs::canonicalize(dir)
                .with_context(|| format!("cannot resolve `{}`", dir.display()))?;
            if !real.is_dir() {
                bail!("`{}` is not a directory", real.display());
            }
            Ok(real)
        })
        .collect()
}

/// Read the file behind `raw_path` after every local check.
/// The limit is a parameter: tests pass small ones.
pub async fn load_file(
    operation_id: &str,
    raw_path: &str,
    allowed_dirs: &[PathBuf],
    max_bytes: u64,
) -> Result<UploadFile, ClientError> {
    load(raw_path, allowed_dirs, max_bytes)
        .await
        .map_err(|refusal| ClientError::InvalidArgs {
            operation_id: operation_id.to_string(),
            message: refusal.to_string(),
        })
}

async fn load(
    raw_path: &str,
    allowed_dirs: &[PathBuf],
    max_bytes: u64,
) -> Result<UploadFile, UploadError> {
    if raw_path.len() > MAX_PATH_BYTES {
        return Err(UploadError::PathTooLong {
            limit: MAX_PATH_BYTES,
        });
    }
    let given = Path::new(raw_path);
    if !given.is_absolute() {
        return Err(UploadError::NotAbsolute {
            path: given.to_path_buf(),
        });
    }

    let path = tokio::fs::canonicalize(given)
        .await
        .map_err(|source| UploadError::Unreadable {
            path: given.to_path_buf(),
            source,
        })?;

    if !allowed_dirs.is_empty() && !allowed_dirs.iter().any(|dir| path.starts_with(dir)) {
        let allowed: Vec<String> = allowed_dirs
            .iter()
            .map(|dir| dir.display().to_string())
            .collect();
        return Err(UploadError::OutsideAllowedDirs {
            path,
            allowed: allowed.join(", "),
        });
    }

    // Check the kind first: opening a FIFO blocks.
    let on_disk = tokio::fs::metadata(&path)
        .await
        .map_err(|source| UploadError::Unreadable {
            path: path.clone(),
            source,
        })?;
    if !on_disk.is_file() {
        return Err(UploadError::NotRegularFile { path });
    }

    let file = File::open(&path)
        .await
        .map_err(|source| UploadError::CannotOpen {
            path: path.clone(),
            source,
        })?;
    let length = check_open_file(&file, max_bytes).await?;
    let bytes = read_bounded(file, length, max_bytes).await?;

    let content_type =
        detect_content_type(&bytes).ok_or_else(|| UploadError::UnrecognisedType {
            first_bytes: bytes
                .iter()
                .take(SIGNATURE_PREVIEW_BYTES)
                .copied()
                .collect(),
        })?;
    let name = given
        .file_name()
        .or_else(|| path.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| raw_path.to_string());

    Ok(UploadFile {
        path,
        name,
        content_type,
        bytes: Bytes::from(bytes),
    })
}

/// Size and kind as seen through the open descriptor.
/// A concurrent writer cannot swap that view.
pub async fn check_open_file(file: &File, max_bytes: u64) -> Result<u64, UploadError> {
    let meta = file.metadata().await.map_err(UploadError::CannotStat)?;
    if !meta.is_file() {
        return Err(UploadError::OpenedNotRegularFile);
    }
    if meta.len() > max_bytes {
        return Err(UploadError::TooLarge {
            size: meta.len(),
            limit: max_bytes,
        });
    }
    Ok(meta.len())
}

/// Read at most `max_bytes + 1` bytes.
/// A file that grew since the check is refused.
pub async fn read_bounded(file: File, length: u64, max_bytes: u64) -> Result<Vec<u8>, UploadError> {
    let mut bytes = Vec::with_capacity(usize::try_from(length).unwrap_or(0));
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .await
        .map_err(UploadError::CannotRead)?;
    if bytes.len() as u64 > max_bytes {
        return Err(UploadError::GrewWhileReading { limit: max_bytes });
    }
    Ok(bytes)
}

/// Content type read from the bytes, never the extension.
/// Covers the types Pennylane accepts for attachments.
#[must_use]
pub fn detect_content_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
        return Some("image/tiff");
    }
    if is_bmp(bytes) {
        return Some("image/bmp");
    }
    // The PDF specification tolerates a preamble before the header.
    let window = &bytes[..bytes.len().min(PDF_SIGNATURE_WINDOW)];
    if window.windows(5).any(|w| w == b"%PDF-") {
        return Some("application/pdf");
    }
    None
}

/// Check the `BM` magic, reserved words and DIB size.
/// The magic alone would admit any text starting with BM.
fn is_bmp(bytes: &[u8]) -> bool {
    let has_magic = bytes.starts_with(b"BM");
    let reserved_is_zero = bytes.get(6..10) == Some(&[0, 0, 0, 0][..]);
    let dib_size = bytes
        .get(14..18)
        .and_then(|field| <[u8; 4]>::try_from(field).ok())
        .map(u32::from_le_bytes);
    let known_dib_size = matches!(dib_size, Some(12 | 16 | 40 | 52 | 56 | 64 | 108 | 124));
    has_magic && reserved_is_zero && known_dib_size
}

fn signature_preview(first_bytes: &[u8]) -> String {
    if first_bytes.is_empty() {
        return "empty file".to_string();
    }
    let hex: Vec<String> = first_bytes.iter().map(|b| format!("{:02x}", b)).collect();
    hex.join(" ")
}

/// Length counts characters, as the OpenAPI `maxLength` does.
/// Control characters are refused: the name enters a part header.
pub fn check_filename(name: &str) -> Result<(), UploadError> {
    if name.is_empty() {
        return Err(UploadError::EmptyFilename);
    }
    let length = name.chars().count();
    if length > MAX_FILENAME_CHARS {
        return Err(UploadError::FilenameTooLong {
            length,
            limit: MAX_FILENAME_CHARS,
        });
    }
    if name.chars().any(char::is_control) {
        return Err(UploadError::FilenameControlCharacters);
    }
    Ok(())
}

/// Turn a multipart operation's body into a form.
/// Fields are checked against the schema before reading the file.
/// `body` is what `split_params` returned.
pub async fn prepare(
    tool: &GeneratedTool,
    body: Option<Value>,
    allowed_dirs: &[PathBuf],
    max_bytes: u64,
) -> Result<PreparedUpload, ClientError> {
    let invalid = |message: String| ClientError::InvalidArgs {
        operation_id: tool.name.to_string(),
        message,
    };
    let [file_field] = tool.binary_fields else {
        return Err(invalid(
            "this operation does not take exactly one file".to_string(),
        ));
    };

    let mut fields = match body {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    };
    let schema: Value =
        serde_json::from_str(tool.input_schema_json).unwrap_or(Value::Object(Map::new()));
    let empty = Map::new();
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .unwrap_or(&empty);

    if let Some(unknown) = fields.keys().find(|k| !properties.contains_key(k.as_str())) {
        let accepted: Vec<&str> = properties.keys().map(String::as_str).collect();
        return Err(invalid(format!(
            "unknown field `{}`; accepted fields: {}",
            unknown,
            accepted.join(", ")
        )));
    }
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|name| !tool.path_template.contains(&format!("{{{}}}", name)));
    for name in required {
        if !matches!(fields.get(name), Some(value) if !value.is_null()) {
            return Err(invalid(format!("missing required field `{}`", name)));
        }
    }

    let raw_path = match fields.remove(*file_field) {
        Some(Value::String(path)) => path,
        _ => {
            return Err(invalid(format!(
                "`{}` must be the absolute path of a local file",
                file_field
            )))
        }
    };
    let explicit_name = match fields.remove("filename") {
        Some(Value::String(name)) => Some(name),
        Some(Value::Null) | None => None,
        Some(_) => return Err(invalid("`filename` must be a string".to_string())),
    };

    let mut parts = Vec::new();
    for (name, value) in fields {
        let property = properties.get(&name);
        let is_object =
            property.and_then(|p| p.get("type")).and_then(Value::as_str) == Some("object");
        let part = match value {
            Value::Null => continue,
            Value::Object(_) if is_object => FormPart::Json {
                json: value.to_string(),
                name,
            },
            _ if is_object => return Err(invalid(format!("`{}` must be a JSON object", name))),
            Value::String(text) => FormPart::Text { name, value: text },
            Value::Number(number) => FormPart::Text {
                name,
                value: number.to_string(),
            },
            Value::Bool(flag) => FormPart::Text {
                name,
                value: flag.to_string(),
            },
            _ => return Err(invalid(format!("`{}` must be a string", name))),
        };
        check_part(&part, property).map_err(|refusal| invalid(refusal.to_string()))?;
        parts.push(part);
    }

    let file = load_file(tool.name, &raw_path, allowed_dirs, max_bytes).await?;
    let file_name = explicit_name.unwrap_or(file.name);
    check_filename(&file_name).map_err(|refusal| invalid(refusal.to_string()))?;

    // Part filename encoding varies between servers.
    // Operations declaring `filename` also get it as UTF-8 text.
    if properties.contains_key("filename") {
        parts.push(FormPart::Text {
            name: "filename".to_string(),
            value: file_name.clone(),
        });
    }

    Ok(PreparedUpload {
        body: MultipartBody {
            parts,
            file_field: file_field.to_string(),
            file_name,
            content_type: file.content_type,
            bytes: file.bytes,
        },
        path: file.path,
    })
}

fn check_part(part: &FormPart, property: Option<&Value>) -> Result<(), UploadError> {
    let (name, text) = match part {
        FormPart::Text { name, value } => (name, value),
        FormPart::Json { name, json } => (name, json),
    };
    if text.len() > MAX_FIELD_BYTES {
        return Err(UploadError::FieldTooLarge {
            name: name.clone(),
            size: text.len(),
            limit: MAX_FIELD_BYTES,
        });
    }
    let allowed = property
        .and_then(|p| p.get("enum"))
        .and_then(Value::as_array);
    if let (FormPart::Text { value, .. }, Some(allowed)) = (part, allowed) {
        if !allowed.iter().any(|v| v.as_str() == Some(value.as_str())) {
            return Err(UploadError::FieldNotAllowed {
                name: name.clone(),
                allowed: allowed.iter().map(Value::to_string).collect(),
            });
        }
    }
    Ok(())
}

/// What left the machine, merged into the response.
/// It shows which local file was sent.
pub fn audit(upload: &PreparedUpload) -> Value {
    json!({
        "path": upload.path.display().to_string(),
        "filename": upload.body.file_name,
        "size_bytes": upload.body.bytes.len(),
        "content_type": upload.body.content_type,
    })
}
