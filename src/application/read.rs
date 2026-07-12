use super::ports::ApiFailureKind;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadFailureKind {
    Credential,
    Api(ApiFailureKind),
    PaginationContract,
    Internal,
}
