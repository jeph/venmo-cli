use serde::Deserialize;

#[derive(Default, Deserialize)]
pub(crate) struct PaginationDto {
    #[serde(default)]
    pub next: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum StringOrInteger {
    String(String),
    Unsigned(u64),
    Signed(i64),
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum StringOrNumber {
    String(String),
    Number(serde_json::Number),
}

impl StringOrNumber {
    pub(crate) fn into_string(self) -> String {
        match self {
            Self::String(value) => value,
            Self::Number(value) => value.to_string(),
        }
    }

    pub(crate) fn as_str(&self) -> std::borrow::Cow<'_, str> {
        match self {
            Self::String(value) => std::borrow::Cow::Borrowed(value),
            Self::Number(value) => std::borrow::Cow::Owned(value.to_string()),
        }
    }
}

impl StringOrInteger {
    pub(crate) fn into_string(self) -> String {
        match self {
            Self::String(value) => value,
            Self::Unsigned(value) => value.to_string(),
            Self::Signed(value) => value.to_string(),
        }
    }

    pub(crate) fn as_str(&self) -> std::borrow::Cow<'_, str> {
        match self {
            Self::String(value) => std::borrow::Cow::Borrowed(value),
            Self::Unsigned(value) => std::borrow::Cow::Owned(value.to_string()),
            Self::Signed(value) => std::borrow::Cow::Owned(value.to_string()),
        }
    }
}
