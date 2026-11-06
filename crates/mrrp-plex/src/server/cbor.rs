use std::fmt::Display;

use axum::{
    extract::{
        FromRequest,
        rejection::BytesRejection,
    },
    http::{
        HeaderValue,
        StatusCode,
        header,
    },
    response::{
        IntoResponse,
        Response,
    },
};
use bytes::{
    BufMut,
    Bytes,
    BytesMut,
};
use serde::{
    Serialize,
    de::DeserializeOwned,
};

pub const CONTENT_TYPE: HeaderValue = HeaderValue::from_static("application/cbor");

#[derive(
    Clone, Copy, Debug, Default, derive_more::Deref, derive_more::DerefMut, derive_more::From,
)]
pub struct Cbor<T>(pub T);

impl<T, S> FromRequest<S> for Cbor<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = CborRejection;

    async fn from_request(req: axum::extract::Request, state: &S) -> Result<Self, Self::Rejection> {
        // check content-type header
        if let Some(content_type) = req.headers().get(header::CONTENT_TYPE) {
            if content_type != &CONTENT_TYPE {
                return Err(CborRejection::ContentType {
                    content_type: Some(content_type.clone()),
                });
            }
        }
        else {
            return Err(CborRejection::ContentType { content_type: None });
        }

        let bytes = Bytes::from_request(req, state).await?;

        let value = ciborium::from_reader(&*bytes).map_err(CborRejection::from_ciborium)?;

        Ok(Self(value))
    }
}

impl<T> IntoResponse for Cbor<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        let mut buffer = BytesMut::with_capacity(128);
        match ciborium::into_writer(&self.0, (&mut buffer).writer()) {
            Ok(()) => ([(header::CONTENT_TYPE, CONTENT_TYPE)], buffer.freeze()).into_response(),
            Err(ciborium::ser::Error::Io(_)) => unreachable!(),
            Err(ciborium::ser::Error::Value(error)) => {
                (StatusCode::INTERNAL_SERVER_ERROR, error).into_response()
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CborRejection {
    #[error("Syntax error at offset {offset}")]
    Syntax { offset: usize },

    #[error("{}", DataErrorFormatter { offset: *offset, description })]
    Data {
        offset: Option<usize>,
        description: String,
    },

    #[error("Recursion limit exceeded")]
    RecursionLimitExceeded,

    #[error("{}", ContentTypeErrorFomatter { content_type: content_type.as_ref() })]
    ContentType { content_type: Option<HeaderValue> },

    #[error(transparent)]
    BytesRejection(#[from] BytesRejection),
}

impl CborRejection {
    fn from_ciborium(error: ciborium::de::Error<std::io::Error>) -> Self {
        match error {
            ciborium::de::Error::Io(_) => {
                // we deserialize from &[u8], so this never happends
                unreachable!()
            }
            ciborium::de::Error::Syntax(offset) => Self::Syntax { offset },
            ciborium::de::Error::Semantic(offset, description) => {
                Self::Data {
                    offset,
                    description,
                }
            }
            ciborium::de::Error::RecursionLimitExceeded => Self::RecursionLimitExceeded,
        }
    }
}

impl IntoResponse for CborRejection {
    fn into_response(self) -> Response {
        match self {
            CborRejection::Syntax { offset: _ } => {
                (
                    StatusCode::BAD_REQUEST,
                    "Failed to parse the request body as CBOR",
                )
                    .into_response()
            }
            CborRejection::Data {
                offset: _,
                description: _,
            } => {
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "Failed to deserialize the CBOR body into the target type",
                )
                    .into_response()
            }
            CborRejection::RecursionLimitExceeded => {
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "Recursion limit exceeded while deserializing CBOR body",
                )
                    .into_response()
            }
            CborRejection::ContentType { content_type: _ } => {
                (
                    StatusCode::UNSUPPORTED_MEDIA_TYPE,
                    "Expected request with `Content-Type: application/cbor`",
                )
                    .into_response()
            }
            CborRejection::BytesRejection(bytes_rejection) => bytes_rejection.into_response(),
        }
    }
}

struct DataErrorFormatter<'a> {
    offset: Option<usize>,
    description: &'a str,
}

impl<'a> Display for DataErrorFormatter<'a> {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(offset) = self.offset {
            write!(f, "Data error at offset {offset}: {}", self.description)
        }
        else {
            write!(f, "Data error: {}", self.description)
        }
    }
}

struct ContentTypeErrorFomatter<'a> {
    content_type: Option<&'a HeaderValue>,
}

impl<'a> Display for ContentTypeErrorFomatter<'a> {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(content_type) = self.content_type {
            write!(
                f,
                "Unexpected content-type: `{content_type:?}`. Expected `{CONTENT_TYPE:?}`"
            )
        }
        else {
            write!(f, "Missing content-type. Expected `{CONTENT_TYPE:?}`",)
        }
    }
}
