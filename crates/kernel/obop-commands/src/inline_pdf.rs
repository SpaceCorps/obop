//! The one policy for showing a PDF inside a page (D92): a stored file is served inline only when
//! its stored type is exactly `application/pdf` (the type was decided by the first bytes when it
//! was uploaded, never by the name or the client) and it is not larger than the field limit of a
//! file, and then under a name we choose. Everything else stays a download.
//!
//! The headers that go with it are in `obop-http` (`inline_pdf_response`). Used by the signer
//! door for the held copy of one request. The back office does not use this: it makes a typed
//! blob in the browser from the authenticated download.

use crate::blobs::{BlobError, Served};

/// The largest file shown inline: the cap of a `file` field.
pub const MAX_INLINE_PDF: usize = 25 * 1024 * 1024;

/// The name an inline PDF is served under. The upload's own name can hold a person's name.
pub const INLINE_PDF_NAME: &str = "document.pdf";

/// `served` as an inline PDF, or `Unsupported` when it is not one (wrong type, too large, or bytes
/// that do not start like a PDF).
pub fn inline_pdf(mut served: Served) -> Result<Served, BlobError> {
    if served.content_type != "application/pdf" {
        return Err(BlobError::Unsupported(
            "only a PDF is shown inline".to_string(),
        ));
    }
    if served.bytes.len() > MAX_INLINE_PDF {
        return Err(BlobError::TooLarge(MAX_INLINE_PDF));
    }
    if !served.bytes.starts_with(b"%PDF-") {
        return Err(BlobError::Unsupported(
            "the file does not start like a PDF".to_string(),
        ));
    }
    served.attachment = false;
    served.filename = INLINE_PDF_NAME.to_string();
    Ok(served)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn served(ct: &str, bytes: &[u8]) -> Served {
        Served {
            content_type: ct.into(),
            bytes: bytes.to_vec(),
            etag: "\"x\"".into(),
            filename: "Jane Q. Public NDA.pdf".into(),
            attachment: true,
            width: None,
            height: None,
        }
    }

    #[test]
    fn a_pdf_is_served_inline_under_our_name_and_nothing_else_is() {
        let ok = inline_pdf(served("application/pdf", b"%PDF-1.4 made up")).unwrap();
        assert!(!ok.attachment);
        assert_eq!(ok.filename, "document.pdf");
        for ct in ["text/html", "image/png", "application/pdf; charset=x", "", "APPLICATION/PDF"] {
            assert!(
                matches!(
                    inline_pdf(served(ct, b"%PDF-1.4 made up")),
                    Err(BlobError::Unsupported(_))
                ),
                "{ct:?}"
            );
        }
        // The stored type says PDF but the bytes are something else.
        assert!(inline_pdf(served("application/pdf", b"<html>")).is_err());
        let mut big = b"%PDF-1.4".to_vec();
        big.resize(MAX_INLINE_PDF + 1, b' ');
        assert!(matches!(
            inline_pdf(served("application/pdf", &big)),
            Err(BlobError::TooLarge(_))
        ));
        big.truncate(MAX_INLINE_PDF);
        assert!(inline_pdf(served("application/pdf", &big)).is_ok());
    }
}
