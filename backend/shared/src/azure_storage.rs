use base64::Engine as _;
use hmac::Mac;
use url::Url;

pub fn service_endpoint(
    account: &str,
    emulator_host: Option<&str>,
    port: u16,
    endpoint: &str,
) -> Result<Url, AzureStorageError> {
    let endpoint = match emulator_host {
        Some(host) => {
            let mut endpoint =
                Url::parse("http://localhost/").map_err(AzureStorageError::Endpoint)?;
            endpoint
                .set_host(Some(host.trim_end_matches('/')))
                .map_err(|_| AzureStorageError::InvalidEmulatorHost {
                    host: host.to_owned(),
                })?;
            endpoint
                .set_port(Some(port))
                .map_err(|_| AzureStorageError::InvalidEmulatorHost {
                    host: host.to_owned(),
                })?;
            endpoint.set_path(&format!("/{account}"));
            endpoint.to_string()
        }
        None => format!("https://{account}.{endpoint}.core.windows.net/"),
    };

    Url::parse(&endpoint).map_err(AzureStorageError::Endpoint)
}

pub fn blob_url(
    endpoint: &Url,
    container_name: &str,
    blob_path: &str,
) -> Result<Url, AzureStorageError> {
    let mut url = endpoint.clone();

    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| AzureStorageError::CannotBeBaseUrl)?;

        segments.push(container_name);

        for segment in blob_path.split('/') {
            segments.push(segment);
        }
    }

    Ok(url)
}

pub fn shared_key_authorization(
    account: &str,
    access_key: &str,
    string_to_sign: &str,
) -> Result<String, AzureStorageError> {
    let access_key = base64::engine::general_purpose::STANDARD
        .decode(access_key)
        .map_err(AzureStorageError::DecodeAccessKey)?;

    let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(&access_key)
        .map_err(AzureStorageError::SignKey)?;

    mac.update(string_to_sign.as_bytes());

    let signature = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());

    Ok(format!("SharedKey {account}:{signature}"))
}

#[derive(Debug, Clone, Copy)]
pub struct SharedKeyRequest<'a> {
    pub account: &'a str,
    pub access_key: &'a str,
    pub method: &'a str,
    pub request_path: &'a str,
    pub content_length: Option<usize>,
    pub content_type: Option<&'a str>,
    pub canonicalized_query: &'a [&'a str],
    pub additional_canonicalized_headers: &'a [&'a str],
    pub x_ms_date: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct StorageStringToSignRequest<'a> {
    pub account: &'a str,
    pub method: &'a str,
    pub request_path: &'a str,
    pub content_length: Option<usize>,
    pub content_type: Option<&'a str>,
    pub canonicalized_query: &'a [&'a str],
    pub additional_canonicalized_headers: &'a [&'a str],
    pub x_ms_date: &'a str,
}

pub fn shared_key_authorization_header(
    request: SharedKeyRequest<'_>,
) -> Result<String, AzureStorageError> {
    let string_to_sign = storage_string_to_sign_with_headers(StorageStringToSignRequest {
        account: request.account,
        method: request.method,
        request_path: request.request_path,
        content_length: request.content_length,
        content_type: request.content_type,
        canonicalized_query: request.canonicalized_query,
        additional_canonicalized_headers: request.additional_canonicalized_headers,
        x_ms_date: request.x_ms_date,
    });

    shared_key_authorization(request.account, request.access_key, &string_to_sign)
}

pub fn validate_queue_name(queue_name: &str) -> Result<(), AzureStorageError> {
    if !(3..=63).contains(&queue_name.len()) {
        return Err(AzureStorageError::InvalidQueueName {
            queue_name: queue_name.to_owned(),
            reason: "queue names must be between 3 and 63 characters",
        });
    }

    if !queue_name
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(AzureStorageError::InvalidQueueName {
            queue_name: queue_name.to_owned(),
            reason: "queue names must contain only lowercase letters, digits, and hyphens",
        });
    }

    if queue_name.starts_with('-') || queue_name.ends_with('-') || queue_name.contains("--") {
        return Err(AzureStorageError::InvalidQueueName {
            queue_name: queue_name.to_owned(),
            reason: "queue names must start and end with a letter or digit and avoid consecutive hyphens",
        });
    }

    Ok(())
}

pub fn canonicalized_resource(
    account: &str,
    request_path: &str,
    canonicalized_query: &[&str],
) -> String {
    let mut value = format!("/{account}{request_path}");

    for query in canonicalized_query {
        value.push('\n');
        value.push_str(query);
    }

    value
}

pub fn storage_string_to_sign(
    account: &str,
    method: &str,
    request_path: &str,
    content_length: Option<usize>,
    content_type: Option<&str>,
    canonicalized_query: &[&str],
    x_ms_date: &str,
) -> String {
    storage_string_to_sign_with_headers(StorageStringToSignRequest {
        account,
        method,
        request_path,
        content_length,
        content_type,
        canonicalized_query,
        additional_canonicalized_headers: &[],
        x_ms_date,
    })
}

pub fn storage_string_to_sign_with_headers(request: StorageStringToSignRequest<'_>) -> String {
    let content_length = request
        .content_length
        .filter(|length| *length > 0)
        .map(|length| length.to_string())
        .unwrap_or_default();
    let content_type = request.content_type.unwrap_or_default();
    let mut canonicalized_headers = String::new();

    for header in request.additional_canonicalized_headers {
        canonicalized_headers.push_str(header);
        canonicalized_headers.push('\n');
    }

    canonicalized_headers.push_str(&format!(
        "x-ms-date:{}\nx-ms-version:2023-11-03\n",
        request.x_ms_date
    ));
    let canonicalized_resource = canonicalized_resource(
        request.account,
        request.request_path,
        request.canonicalized_query,
    );

    format!(
        "{}\n\
        \n\
        \n\
        {content_length}\n\
        \n\
        {content_type}\n\
        \n\
        \n\
        \n\
        \n\
        \n\
        \n\
        {canonicalized_headers}\
        {canonicalized_resource}",
        request.method
    )
}

pub fn queue_message_body(message_text: &str) -> String {
    format!(
        "<QueueMessage><MessageText>{}</MessageText></QueueMessage>",
        escape_xml_text(message_text)
    )
}

pub fn escape_xml_text(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());

    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '\"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(character),
        }
    }

    escaped
}

#[derive(Debug, thiserror::Error)]
pub enum AzureStorageError {
    #[error("azure storage error: endpoint cannot be used as base URL")]
    CannotBeBaseUrl,
    #[error("azure storage error: failed to decode storage access key: {0}")]
    DecodeAccessKey(base64::DecodeError),
    #[error("azure storage error: failed to build storage endpoint: {0}")]
    Endpoint(url::ParseError),
    #[error("azure storage error: invalid storage emulator host '{host}'")]
    InvalidEmulatorHost { host: String },
    #[error("azure storage error: invalid queue name '{queue_name}': {reason}")]
    InvalidQueueName {
        queue_name: String,
        reason: &'static str,
    },
    #[error("azure storage error: failed to create storage request signature: {0}")]
    SignKey(hmac::digest::InvalidLength),
}

#[cfg(test)]
mod tests {
    use super::{
        blob_url, canonicalized_resource, escape_xml_text, queue_message_body, service_endpoint,
        storage_string_to_sign, storage_string_to_sign_with_headers, validate_queue_name,
        StorageStringToSignRequest,
    };

    #[test]
    fn builds_emulator_queue_endpoint() {
        let endpoint =
            service_endpoint("devstoreaccount1", Some("127.0.0.1"), 10001, "queue").unwrap();

        assert_eq!(endpoint.as_str(), "http://127.0.0.1:10001/devstoreaccount1");
    }

    #[test]
    fn builds_azure_queue_endpoint() {
        let endpoint = service_endpoint("lumenstorage", None, 10001, "queue").unwrap();

        assert_eq!(
            endpoint.as_str(),
            "https://lumenstorage.queue.core.windows.net/"
        );
    }

    #[test]
    fn validates_queue_names() {
        validate_queue_name("viirs-processing").unwrap();
        validate_queue_name("viirs-processing-deadletter").unwrap();

        assert!(validate_queue_name("Viirs").is_err());
        assert!(validate_queue_name("-viirs").is_err());
        assert!(validate_queue_name("viirs--processing").is_err());
    }

    #[test]
    fn canonicalized_resource_includes_ordered_query_lines() {
        assert_eq!(
            canonicalized_resource(
                "devstoreaccount1",
                "/devstoreaccount1/viirs-processing/messages",
                &["numofmessages:1", "visibilitytimeout:900"],
            ),
            "/devstoreaccount1/devstoreaccount1/viirs-processing/messages\nnumofmessages:1\nvisibilitytimeout:900"
        );
    }

    #[test]
    fn storage_get_string_to_sign_includes_query_lines() {
        let value = storage_string_to_sign(
            "devstoreaccount1",
            "GET",
            "/devstoreaccount1/viirs-processing/messages",
            None,
            None,
            &["numofmessages:1", "visibilitytimeout:900"],
            "Sun, 24 May 2026 12:00:00 GMT",
        );

        assert!(value.contains("x-ms-date:Sun, 24 May 2026 12:00:00 GMT"));
        assert!(value.contains("x-ms-version:2023-11-03"));
        assert!(value.contains("/devstoreaccount1/devstoreaccount1/viirs-processing/messages"));
        assert!(value.contains("numofmessages:1"));
        assert!(value.contains("visibilitytimeout:900"));
    }

    #[test]
    fn storage_string_to_sign_includes_additional_x_ms_headers_before_date() {
        let value = storage_string_to_sign_with_headers(StorageStringToSignRequest {
            account: "devstoreaccount1",
            method: "PUT",
            request_path: "/devstoreaccount1/raw-viirs/blob.h5",
            content_length: Some(123),
            content_type: None,
            canonicalized_query: &[],
            additional_canonicalized_headers: &["x-ms-blob-type:BlockBlob"],
            x_ms_date: "Sun, 24 May 2026 12:00:00 GMT",
        });

        assert!(value.contains("x-ms-blob-type:BlockBlob\nx-ms-date:Sun, 24 May 2026 12:00:00 GMT"));
    }

    #[test]
    fn storage_post_string_to_sign_includes_content_headers() {
        let value = storage_string_to_sign(
            "devstoreaccount1",
            "POST",
            "/devstoreaccount1/viirs-processing/messages",
            Some(123),
            Some("application/xml"),
            &[],
            "Sun, 24 May 2026 12:00:00 GMT",
        );

        assert!(value.contains("\n123\n"));
        assert!(value.contains("\napplication/xml\n"));
        assert!(value.contains("/devstoreaccount1/devstoreaccount1/viirs-processing/messages"));
    }

    #[test]
    fn escapes_queue_message_xml_text() {
        assert_eq!(
            escape_xml_text("a&b<c>d\"e'f"),
            "a&amp;b&lt;c&gt;d&quot;e&apos;f"
        );
    }

    #[test]
    fn queue_message_body_wraps_escaped_text() {
        assert_eq!(
            queue_message_body("a&b<c>d\"e'f"),
            "<QueueMessage><MessageText>a&amp;b&lt;c&gt;d&quot;e&apos;f</MessageText></QueueMessage>"
        );
    }

    #[test]
    fn builds_emulator_blob_url() {
        let endpoint =
            service_endpoint("devstoreaccount1", Some("127.0.0.1"), 10000, "blob").unwrap();

        let url = blob_url(&endpoint, "raw-viirs", "VNP46A2/2026-05-21/h11v06.h5").unwrap();

        assert_eq!(
            url.as_str(),
            "http://127.0.0.1:10000/devstoreaccount1/raw-viirs/VNP46A2/2026-05-21/h11v06.h5"
        );
    }

    #[test]
    fn builds_azure_blob_url() {
        let endpoint = service_endpoint("lumenstorage", None, 10000, "blob").unwrap();

        let url = blob_url(&endpoint, "raw-viirs", "VNP46A2/2026-05-21/h11v06.h5").unwrap();

        assert_eq!(
            url.as_str(),
            "https://lumenstorage.blob.core.windows.net/raw-viirs/VNP46A2/2026-05-21/h11v06.h5"
        );
    }
}
