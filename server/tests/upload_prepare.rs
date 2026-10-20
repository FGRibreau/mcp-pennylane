//! Multipart params become a form, as in the server.
//! Real files, no network.

mod common;

use serde_json::{json, Value};

use mcp_pennylane::client::FormPart;
use mcp_pennylane::error::ClientError;
use mcp_pennylane::tools::{lookup_tool, split_params};
use mcp_pennylane::upload::{audit, prepare, PreparedUpload, MAX_UPLOAD_BYTES};

use common::{minimal_pdf, TempDir};

async fn prepare_params(
    operation_id: &str,
    params: Value,
) -> Result<(String, PreparedUpload), ClientError> {
    let tool = lookup_tool(operation_id).expect("operation present");
    let params = params.as_object().cloned().expect("params are an object");
    let (path, _, body) = split_params(tool, &params).expect("split");
    let upload = prepare(tool, body, &[], MAX_UPLOAD_BYTES).await?;
    Ok((path, upload))
}

fn refusal(outcome: Result<(String, PreparedUpload), ClientError>) -> String {
    match outcome {
        Err(ClientError::InvalidArgs { message, .. }) => message,
        Err(other) => panic!("expected InvalidArgs, got {:?}", other),
        Ok(_) => panic!("expected a refusal"),
    }
}

fn text(name: &str, value: &str) -> FormPart {
    FormPart::Text {
        name: name.to_string(),
        value: value.to_string(),
    }
}

#[tokio::test]
async fn file_attachment_sends_the_given_filename_as_part_name_and_utf8_text() {
    let dir = TempDir::new("attachment");
    let path = dir.write("scan0001.pdf", &minimal_pdf());
    let (url, upload) = prepare_params(
        "postFileAttachments",
        json!({"file": path.display().to_string(), "filename": "Facture été août.pdf"}),
    )
    .await
    .expect("prepared");
    assert_eq!(url, "/api/external/v2/file_attachments");
    assert_eq!(upload.body.file_field, "file");
    assert_eq!(upload.body.file_name, "Facture été août.pdf");
    assert_eq!(upload.body.content_type, "application/pdf");
    assert_eq!(
        upload.body.parts,
        vec![text("filename", "Facture été août.pdf")]
    );
}

#[tokio::test]
async fn file_attachment_without_filename_falls_back_to_the_local_name_in_both_places() {
    let dir = TempDir::new("local-name");
    let path = dir.write("Relevé août.pdf", &minimal_pdf());
    let (_, upload) = prepare_params(
        "postFileAttachments",
        json!({"file": path.display().to_string()}),
    )
    .await
    .expect("prepared");
    assert_eq!(upload.body.file_name, "Relevé août.pdf");
    assert_eq!(upload.body.parts, vec![text("filename", "Relevé août.pdf")]);
}

#[tokio::test]
async fn ledger_attachment_declares_filename_too() {
    let dir = TempDir::new("ledger");
    let path = dir.write("piece.pdf", &minimal_pdf());
    let (_, upload) = prepare_params(
        "postLedgerAttachments",
        json!({"file": path.display().to_string()}),
    )
    .await
    .expect("prepared");
    assert_eq!(upload.body.parts, vec![text("filename", "piece.pdf")]);
}

#[tokio::test]
async fn appendix_takes_its_id_from_the_path_and_sends_no_filename_field() {
    let dir = TempDir::new("appendix");
    let path = dir.write("annexe.pdf", &minimal_pdf());
    let (url, upload) = prepare_params(
        "postQuoteAppendices",
        json!({"quote_id": 1234, "file": path.display().to_string()}),
    )
    .await
    .expect("prepared");
    assert_eq!(url, "/api/external/v2/quotes/1234/appendices");
    assert_eq!(upload.body.file_name, "annexe.pdf");
    assert!(upload.body.parts.is_empty());
}

#[tokio::test]
async fn operation_that_does_not_declare_filename_refuses_it() {
    let dir = TempDir::new("no-filename");
    let path = dir.write("annexe.pdf", &minimal_pdf());
    let message = refusal(
        prepare_params(
            "postQuoteAppendices",
            json!({"quote_id": 1, "file": path.display().to_string(), "filename": "autre.pdf"}),
        )
        .await,
    );
    assert!(message.contains("unknown field `filename`"), "{message}");
    assert!(message.contains("accepted fields"), "{message}");
}

#[tokio::test]
async fn unknown_field_is_refused_before_the_file_is_looked_at() {
    let message = refusal(
        prepare_params(
            "postFileAttachments",
            json!({"file": "/nonexistent/never-read.pdf", "label": "x"}),
        )
        .await,
    );
    assert!(message.contains("unknown field `label`"), "{message}");
}

#[tokio::test]
async fn missing_file_field_is_named() {
    let message = refusal(prepare_params("postFileAttachments", json!({})).await);
    assert!(
        message.contains("missing required field `file`"),
        "{message}"
    );
}

#[tokio::test]
async fn file_must_be_a_path_string() {
    let message = refusal(prepare_params("postFileAttachments", json!({"file": 12})).await);
    assert!(
        message.contains("absolute path of a local file"),
        "{message}"
    );
}

#[tokio::test]
async fn filename_longer_than_255_characters_is_refused() {
    let dir = TempDir::new("long-name");
    let path = dir.write("ok.pdf", &minimal_pdf());
    let message = refusal(
        prepare_params(
            "postFileAttachments",
            json!({"file": path.display().to_string(), "filename": "a".repeat(256)}),
        )
        .await,
    );
    assert!(message.contains("256 characters"), "{message}");
}

#[tokio::test]
async fn filename_with_a_line_break_is_refused() {
    let dir = TempDir::new("injected-name");
    let path = dir.write("ok.pdf", &minimal_pdf());
    let message = refusal(
        prepare_params(
            "postFileAttachments",
            json!({"file": path.display().to_string(), "filename": "a.pdf\r\nX-Injected: 1"}),
        )
        .await,
    );
    assert!(message.contains("control characters"), "{message}");
}

#[tokio::test]
async fn factur_x_options_travel_as_a_json_part() {
    let dir = TempDir::new("facturx");
    let path = dir.write("facture-fournisseur.pdf", &minimal_pdf());
    let options = json!({
        "supplier_id": 12,
        "invoice_lines": [{"e_invoice_line_id": "1", "ledger_account_id": 123}]
    });
    let (url, upload) = prepare_params(
        "createSupplierInvoiceEInvoiceImport",
        json!({"file": path.display().to_string(), "invoice_options": options.clone()}),
    )
    .await
    .expect("prepared");
    assert_eq!(url, "/api/external/v2/supplier_invoices/e_invoices/imports");
    match upload.body.parts.as_slice() {
        [FormPart::Json { name, json }] => {
            assert_eq!(name, "invoice_options");
            assert_eq!(serde_json::from_str::<Value>(json).expect("json"), options);
        }
        other => panic!("expected one JSON part, got {:?}", other),
    }
}

#[tokio::test]
async fn factur_x_without_options_sends_only_the_file() {
    let dir = TempDir::new("facturx-bare");
    let path = dir.write("facture.pdf", &minimal_pdf());
    let (_, upload) = prepare_params(
        "createSupplierInvoiceEInvoiceImport",
        json!({"file": path.display().to_string()}),
    )
    .await
    .expect("prepared");
    assert!(upload.body.parts.is_empty());
}

#[tokio::test]
async fn invoice_options_must_be_an_object() {
    let dir = TempDir::new("facturx-string");
    let path = dir.write("facture.pdf", &minimal_pdf());
    let message = refusal(
        prepare_params(
            "createSupplierInvoiceEInvoiceImport",
            json!({"file": path.display().to_string(), "invoice_options": "{\"supplier_id\":12}"}),
        )
        .await,
    );
    assert!(
        message.contains("`invoice_options` must be a JSON object"),
        "{message}"
    );
}

#[tokio::test]
async fn e_invoice_import_needs_a_type_from_its_enum_before_anything_is_read() {
    let missing = refusal(
        prepare_params(
            "createEInvoiceImport",
            json!({"file": "/nonexistent/never-read.pdf"}),
        )
        .await,
    );
    assert!(
        missing.contains("missing required field `type`"),
        "{missing}"
    );

    let wrong = refusal(
        prepare_params(
            "createEInvoiceImport",
            json!({"file": "/nonexistent/never-read.pdf", "type": "other"}),
        )
        .await,
    );
    assert!(wrong.contains("`type` must be one of"), "{wrong}");
    assert!(wrong.contains("\"supplier\""), "{wrong}");

    let dir = TempDir::new("einvoice");
    let path = dir.write("facture.pdf", &minimal_pdf());
    let (_, upload) = prepare_params(
        "createEInvoiceImport",
        json!({"file": path.display().to_string(), "type": "supplier"}),
    )
    .await
    .expect("prepared");
    assert_eq!(upload.body.parts, vec![text("type", "supplier")]);
}

#[tokio::test]
async fn oversized_text_field_is_refused() {
    let dir = TempDir::new("big-field");
    let path = dir.write("facture.pdf", &minimal_pdf());
    let message = refusal(
        prepare_params(
            "createSupplierInvoiceEInvoiceImport",
            json!({
                "file": path.display().to_string(),
                "invoice_options": {"supplier_id": 1, "note": "x".repeat(1_100_000)}
            }),
        )
        .await,
    );
    assert!(message.contains("`invoice_options` is"), "{message}");
}

#[tokio::test]
async fn audit_names_what_was_read() {
    let dir = TempDir::new("audit");
    let pdf = minimal_pdf();
    let path = dir.write("piece.pdf", &pdf);
    let (_, upload) = prepare_params(
        "postFileAttachments",
        json!({"file": path.display().to_string()}),
    )
    .await
    .expect("prepared");
    let audit = audit(&upload);
    assert_eq!(audit["filename"], "piece.pdf");
    assert_eq!(audit["size_bytes"], pdf.len());
    assert_eq!(audit["content_type"], "application/pdf");
    assert_eq!(
        audit["path"],
        std::fs::canonicalize(&path)
            .expect("canonical")
            .display()
            .to_string()
    );
}
