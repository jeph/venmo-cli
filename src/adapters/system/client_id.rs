use crate::shared::{ClientRequestId, ClientRequestIdGenerator};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SystemClientRequestIdGenerator;

impl ClientRequestIdGenerator for SystemClientRequestIdGenerator {
    fn generate(&self) -> ClientRequestId {
        ClientRequestId::generate()
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::str::FromStr;

    use super::*;

    #[test]
    fn generated_client_request_id_has_canonical_rfc4122_uuid_v4_format()
    -> Result<(), Box<dyn Error>> {
        let id = SystemClientRequestIdGenerator.generate();
        let rendered = id.to_string();
        let parsed = uuid::Uuid::parse_str(&rendered)?;

        assert_eq!(rendered.len(), 36);
        assert_eq!(parsed.hyphenated().to_string(), rendered);
        assert_eq!(parsed.get_version_num(), 4);
        assert_eq!(parsed.get_variant(), uuid::Variant::RFC4122);
        assert_eq!(ClientRequestId::from_str(&rendered)?, id);
        assert!(!format!("{id:?}").contains(&rendered));
        Ok(())
    }
}
