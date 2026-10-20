//! Local guards and preparation for `multipart/form-data` operations.
//! A model picks the path, maybe from untrusted content.
//! Every check runs here, before any byte leaves.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use bytes::Bytes;
use serde_json::{json, Map, Value};
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::client::{FormPart, MultipartBody};
use crate::error::ClientError;
use crate::generated::GeneratedTool;

/// Ceiling Pennylane documents for an uploaded file.
pub const MAX_UPLOAD_BYTES: u64 = 100_000_000;
/// Longest wait for a file to be read from disk.
pub const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// Uploads that may run at once, which bounds memory.
pub const MAX_CONCURRENT_UPLOADS: usize = 4;
/// Longest wait for a free upload slot.
pub const PERMIT_WAIT_TIMEOUT: Duration = Duration::from_secs(30);
pub const MAX_UPLOAD_DIRS: usize = 32;
const MAX_FILENAME_CHARS: usize = 255;
const MAX_PATH_BYTES: usize = 4_096;
const MAX_FIELD_BYTES: usize = 1_048_576;
const PDF_SIGNATURE_WINDOW: usize = 1_024;
const SIGNATURE_PREVIEW_BYTES: usize = 8;
const ACCEPTED_TYPES: &str = "PDF, PNG, JPEG, TIFF, BMP, GIF";
const FILENAME_FIELD: &str = "filename";

/// What is wrong with a form field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldProblem {
    Missing,
    NotAString,
    NotAnObject,
    NotAPath,
}

impl FieldProblem {
    fn describe(self, name: &str) -> String {
        match self {
            FieldProblem::Missing => format!("missing required field `{name}`"),
            FieldProblem::NotAString => format!("`{name}` must be a string"),
            FieldProblem::NotAnObject => format!("`{name}` must be a JSON object"),
            FieldProblem::NotAPath => {
                format!("`{name}` must be the absolute path of a local file")
            }
        }
    }
}

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
    #[error("the file could not be read within {limit:?}: it may still be downloading from a cloud drive, or sit on a network mount that does not answer")]
    ReadTimedOut { limit: Duration },
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
    #[error("this operation does not take exactly one file")]
    NotSingleFileOperation,
    #[error("unknown field `{name}`; accepted fields: {}", .accepted.join(", "))]
    UnknownField { name: String, accepted: Vec<String> },
    #[error("{}", .problem.describe(.name))]
    InvalidField { name: String, problem: FieldProblem },
    #[error("`{name}` is {size} bytes, the limit is {limit} bytes")]
    FieldTooLarge {
        name: String,
        size: usize,
        limit: usize,
    },
    #[error("`{name}` must be one of {}", .allowed.join(", "))]
    FieldNotAllowed { name: String, allowed: Vec<String> },
}

/// Bounds applied to one upload.
#[derive(Debug, Clone, Copy)]
pub struct UploadLimits {
    pub max_bytes: u64,
    pub read_timeout: Duration,
    pub permit_wait: Duration,
}

impl UploadLimits {
    pub const DEFAULT: UploadLimits = UploadLimits {
        max_bytes: MAX_UPLOAD_BYTES,
        read_timeout: READ_TIMEOUT,
        permit_wait: PERMIT_WAIT_TIMEOUT,
    };
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

/// Wait for one of the upload slots.
/// The slot is held until the permit is dropped.
pub async fn acquire_upload_permit(
    uploads: &Arc<Semaphore>,
    wait: Duration,
) -> Result<OwnedSemaphorePermit, ClientError> {
    match tokio::time::timeout(wait, Arc::clone(uploads).acquire_owned()).await {
        Ok(Ok(permit)) => Ok(permit),
        // Never closed: only the wait can fail.
        _ => Err(ClientError::UploadsBusy {
            limit: MAX_CONCURRENT_UPLOADS,
            waited: wait,
        }),
    }
}

fn invalid_args(operation_id: &str, refusal: &UploadError) -> ClientError {
    ClientError::InvalidArgs {
        operation_id: operation_id.to_string(),
        message: refusal.to_string(),
    }
}

/// Read the file behind `raw_path` after every local check.
/// The limits are a parameter: tests pass small ones.
pub async fn load_file(
    operation_id: &str,
    raw_path: &str,
    allowed_dirs: &[PathBuf],
    limits: UploadLimits,
) -> Result<UploadFile, ClientError> {
    let reading = load(raw_path, allowed_dirs, limits.max_bytes);
    let outcome = match tokio::time::timeout(limits.read_timeout, reading).await {
        Ok(read) => read,
        Err(_elapsed) => Err(UploadError::ReadTimedOut {
            limit: limits.read_timeout,
        }),
    };
    outcome.map_err(|refusal| invalid_args(operation_id, &refusal))
}

async fn load(
    raw_path: &str,
    allowed_dirs: &[PathBuf],
    max_bytes: u64,
) -> Result<UploadFile, UploadError> {
    let given = absolute_path(raw_path)?;
    let path = resolve(given).await?;
    check_allowed(&path, allowed_dirs)?;
    ensure_regular_file(&path).await?;
    let bytes = read_file(&path, max_bytes).await?;
    let content_type = recognised_type(&bytes)?;
    Ok(UploadFile {
        name: display_name(given, &path, raw_path),
        path,
        content_type,
        bytes: Bytes::from(bytes),
    })
}

fn absolute_path(raw_path: &str) -> Result<&Path, UploadError> {
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
    Ok(given)
}

async fn resolve(given: &Path) -> Result<PathBuf, UploadError> {
    tokio::fs::canonicalize(given)
        .await
        .map_err(|source| UploadError::Unreadable {
            path: given.to_path_buf(),
            source,
        })
}

fn check_allowed(path: &Path, allowed_dirs: &[PathBuf]) -> Result<(), UploadError> {
    let is_inside = |dir: &PathBuf| path.starts_with(dir);
    if allowed_dirs.is_empty() || allowed_dirs.iter().any(is_inside) {
        return Ok(());
    }
    let allowed: Vec<String> = allowed_dirs
        .iter()
        .map(|dir| dir.display().to_string())
        .collect();
    Err(UploadError::OutsideAllowedDirs {
        path: path.to_path_buf(),
        allowed: allowed.join(", "),
    })
}

/// Check the kind first: opening a FIFO blocks.
async fn ensure_regular_file(path: &Path) -> Result<(), UploadError> {
    let on_disk = tokio::fs::metadata(path)
        .await
        .map_err(|source| UploadError::Unreadable {
            path: path.to_path_buf(),
            source,
        })?;
    if on_disk.is_file() {
        return Ok(());
    }
    Err(UploadError::NotRegularFile {
        path: path.to_path_buf(),
    })
}

async fn read_file(path: &Path, max_bytes: u64) -> Result<Vec<u8>, UploadError> {
    let file = File::open(path)
        .await
        .map_err(|source| UploadError::CannotOpen {
            path: path.to_path_buf(),
            source,
        })?;
    let length = check_open_file(&file, max_bytes).await?;
    read_bounded(file, length, max_bytes).await
}

fn recognised_type(bytes: &[u8]) -> Result<&'static str, UploadError> {
    detect_content_type(bytes).ok_or_else(|| UploadError::UnrecognisedType {
        first_bytes: bytes
            .iter()
            .take(SIGNATURE_PREVIEW_BYTES)
            .copied()
            .collect(),
    })
}

fn display_name(given: &Path, path: &Path, raw_path: &str) -> String {
    given
        .file_name()
        .or_else(|| path.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| raw_path.to_string())
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

/// The part of the input schema that uploads need.
struct InputSchema {
    properties: Map<String, Value>,
    required: Vec<String>,
}

impl InputSchema {
    fn of(tool: &GeneratedTool) -> Self {
        let schema: Value = serde_json::from_str(tool.input_schema_json).unwrap_or(Value::Null);
        let properties = match schema.get("properties") {
            Some(Value::Object(declared)) => declared.clone(),
            _ => Map::new(),
        };
        let required = match schema.get("required") {
            Some(Value::Array(names)) => names
                .iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect(),
            _ => Vec::new(),
        };
        Self {
            properties,
            required,
        }
    }

    fn declares(&self, name: &str) -> bool {
        self.properties.contains_key(name)
    }

    fn is_object(&self, name: &str) -> bool {
        let declared = self.properties.get(name).and_then(|p| p.get("type"));
        declared.and_then(Value::as_str) == Some("object")
    }

    fn allowed_values(&self, name: &str) -> Option<&Vec<Value>> {
        let declared = self.properties.get(name).and_then(|p| p.get("enum"));
        declared.and_then(Value::as_array)
    }
}

/// A multipart request whose fields passed validation.
/// No file has been read yet.
#[derive(Debug)]
pub struct UploadRequest {
    operation_id: &'static str,
    file_field: &'static str,
    raw_path: String,
    explicit_name: Option<String>,
    parts: Vec<FormPart>,
    declares_filename: bool,
}

/// Validate a multipart body against the input schema.
/// `body` is what `split_params` returned.
pub fn parse_request(
    tool: &GeneratedTool,
    body: Option<Value>,
) -> Result<UploadRequest, ClientError> {
    build_request(tool, body).map_err(|refusal| invalid_args(tool.name, &refusal))
}

fn build_request(tool: &GeneratedTool, body: Option<Value>) -> Result<UploadRequest, UploadError> {
    let [file_field] = tool.binary_fields else {
        return Err(UploadError::NotSingleFileOperation);
    };
    let mut fields = match body {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    };
    let schema = InputSchema::of(tool);
    reject_unknown_fields(&fields, &schema)?;
    require_fields(tool, &schema, &fields)?;
    let raw_path = take_path(&mut fields, file_field)?;
    let explicit_name = take_filename(&mut fields)?;
    Ok(UploadRequest {
        operation_id: tool.name,
        file_field,
        raw_path,
        explicit_name,
        parts: form_parts(fields, &schema)?,
        declares_filename: schema.declares(FILENAME_FIELD),
    })
}

fn invalid_field(name: &str, problem: FieldProblem) -> UploadError {
    UploadError::InvalidField {
        name: name.to_string(),
        problem,
    }
}

fn reject_unknown_fields(
    fields: &Map<String, Value>,
    schema: &InputSchema,
) -> Result<(), UploadError> {
    match fields.keys().find(|name| !schema.declares(name)) {
        Some(name) => Err(UploadError::UnknownField {
            name: name.clone(),
            accepted: schema.properties.keys().cloned().collect(),
        }),
        None => Ok(()),
    }
}

fn is_path_param(tool: &GeneratedTool, name: &str) -> bool {
    tool.path_template.contains(&format!("{{{}}}", name))
}

fn require_fields(
    tool: &GeneratedTool,
    schema: &InputSchema,
    fields: &Map<String, Value>,
) -> Result<(), UploadError> {
    let body_fields = schema.required.iter().filter(|n| !is_path_param(tool, n));
    for name in body_fields {
        if !matches!(fields.get(name.as_str()), Some(value) if !value.is_null()) {
            return Err(invalid_field(name, FieldProblem::Missing));
        }
    }
    Ok(())
}

fn take_path(fields: &mut Map<String, Value>, file_field: &str) -> Result<String, UploadError> {
    match fields.remove(file_field) {
        Some(Value::String(path)) => Ok(path),
        _ => Err(invalid_field(file_field, FieldProblem::NotAPath)),
    }
}

fn take_filename(fields: &mut Map<String, Value>) -> Result<Option<String>, UploadError> {
    match fields.remove(FILENAME_FIELD) {
        Some(Value::String(name)) => Ok(Some(name)),
        Some(Value::Null) | None => Ok(None),
        Some(_) => Err(invalid_field(FILENAME_FIELD, FieldProblem::NotAString)),
    }
}

fn form_parts(
    fields: Map<String, Value>,
    schema: &InputSchema,
) -> Result<Vec<FormPart>, UploadError> {
    let mut parts = Vec::new();
    for (name, value) in fields {
        let Some(part) = form_part(name, value, schema)? else {
            continue;
        };
        check_part(&part, schema)?;
        parts.push(part);
    }
    Ok(parts)
}

fn form_part(
    name: String,
    value: Value,
    schema: &InputSchema,
) -> Result<Option<FormPart>, UploadError> {
    let is_object = schema.is_object(&name);
    let part = match value {
        Value::Null => return Ok(None),
        Value::Object(_) if is_object => FormPart::Json {
            json: value.to_string(),
            name,
        },
        _ if is_object => return Err(invalid_field(&name, FieldProblem::NotAnObject)),
        Value::String(text) => FormPart::Text { name, value: text },
        Value::Number(number) => FormPart::Text {
            name,
            value: number.to_string(),
        },
        Value::Bool(flag) => FormPart::Text {
            name,
            value: flag.to_string(),
        },
        _ => return Err(invalid_field(&name, FieldProblem::NotAString)),
    };
    Ok(Some(part))
}

fn check_part(part: &FormPart, schema: &InputSchema) -> Result<(), UploadError> {
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
    let allowed = schema.allowed_values(name);
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

impl UploadRequest {
    /// Read the file and build the form.
    pub async fn read(
        self,
        allowed_dirs: &[PathBuf],
        limits: UploadLimits,
    ) -> Result<PreparedUpload, ClientError> {
        let operation_id = self.operation_id;
        let file = load_file(operation_id, &self.raw_path, allowed_dirs, limits).await?;
        self.assemble(file)
            .map_err(|refusal| invalid_args(operation_id, &refusal))
    }

    fn assemble(self, file: UploadFile) -> Result<PreparedUpload, UploadError> {
        let file_name = self.explicit_name.unwrap_or(file.name);
        check_filename(&file_name)?;
        let mut parts = self.parts;
        // Part filename encoding varies between servers.
        // Operations declaring `filename` also get it as UTF-8 text.
        if self.declares_filename {
            parts.push(FormPart::Text {
                name: FILENAME_FIELD.to_string(),
                value: file_name.clone(),
            });
        }
        Ok(PreparedUpload {
            body: MultipartBody {
                parts,
                file_field: self.file_field.to_string(),
                file_name,
                content_type: file.content_type,
                bytes: file.bytes,
            },
            path: file.path,
        })
    }
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
