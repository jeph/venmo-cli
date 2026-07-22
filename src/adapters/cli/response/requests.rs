use std::io;

use crate::features::requests::info::RequestInfoResult;
use crate::features::requests::list::RequestsResult;

use super::Response;
use super::shared;

pub(crate) fn requests(result: &RequestsResult) -> io::Result<Response<'_, RequestsResult>> {
    let requests = result
        .requests()
        .iter()
        .map(shared::request)
        .collect::<io::Result<Vec<_>>>()?;
    Ok(Response::new(
        result,
        serde_json::json!({
            "direction": shared::request_direction_filter(result.direction()),
            "requests": requests,
            "next_before": result.next_before().map(|before| before.as_str()),
        }),
    ))
}

pub(crate) fn request_info(
    result: &RequestInfoResult,
) -> io::Result<Response<'_, RequestInfoResult>> {
    Ok(Response::new(
        result,
        serde_json::json!({ "request": shared::request(result.request())? }),
    ))
}
