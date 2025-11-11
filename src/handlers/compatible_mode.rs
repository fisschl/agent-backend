use axum::{
    body::Body,
    extract::{Path, RawQuery, Request, State},
    http::{HeaderMap, Method, StatusCode, header::AUTHORIZATION},
    response::Response,
};

use crate::AppState;

/// 请求头黑名单(需要移除的头)
const REQUEST_HEADERS_BLOCKLIST: &[axum::http::HeaderName] = &[
    axum::http::header::HOST,
    axum::http::header::CONNECTION,
    axum::http::header::TE,
    axum::http::header::TRAILER,
    axum::http::header::TRANSFER_ENCODING,
    axum::http::header::UPGRADE,
    axum::http::header::ORIGIN,
    axum::http::header::REFERER,
];

/// 响应头黑名单(需要移除的头)
const RESPONSE_HEADERS_BLOCKLIST: &[axum::http::HeaderName] = &[
    axum::http::header::CONNECTION,
    axum::http::header::TE,
    axum::http::header::TRAILER,
    axum::http::header::TRANSFER_ENCODING,
    axum::http::header::UPGRADE,
    axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN,
    axum::http::header::ACCESS_CONTROL_ALLOW_METHODS,
    axum::http::header::ACCESS_CONTROL_ALLOW_HEADERS,
    axum::http::header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
    axum::http::header::ACCESS_CONTROL_EXPOSE_HEADERS,
    axum::http::header::ACCESS_CONTROL_MAX_AGE,
];

/// HTTP 兼容模式代理处理器
pub async fn handle_compatible_mode(
    State(state): State<AppState>,
    Path(path): Path<String>,
    RawQuery(query): RawQuery,
    method: Method,
    headers: HeaderMap,
    body: Request,
) -> Result<Response, (StatusCode, String)> {
    let client = &state.http_client;
    // 构建目标URL
    let mut target_url = format!("https://dashscope.aliyuncs.com/compatible-mode/v1/{}", path);

    // 添加查询参数
    if let Some(query_string) = query {
        target_url.push('?');
        target_url.push_str(&query_string);
    }

    // 过滤请求头
    let mut request_headers = HeaderMap::new();
    for (name, value) in headers.iter() {
        if !REQUEST_HEADERS_BLOCKLIST.contains(name) {
            request_headers.insert(name.clone(), value.clone());
        }
    }

    // 使用 AppState 中的 API 密钥设置 Authorization 头(仅当未传入时)
    if !request_headers.contains_key(AUTHORIZATION)
        && let Some(key) = &state.api_key
        && let Ok(auth_value) = axum::http::HeaderValue::from_str(&format!("Bearer {}", key))
    {
        request_headers.insert(AUTHORIZATION, auth_value);
    }

    // 将请求体转换为流
    let body_stream = body.into_body().into_data_stream();

    // 构建请求(流式传输请求体)
    let request_builder = client
        .request(method, &target_url)
        .headers(request_headers)
        .body(reqwest::Body::wrap_stream(body_stream));

    // 发送请求
    let response = request_builder
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;

    // 获取响应状态码
    let status = response.status();

    // 构建响应并过滤响应头
    let mut builder = Response::builder().status(status);
    for (name, value) in response.headers().iter() {
        if !RESPONSE_HEADERS_BLOCKLIST.contains(name) {
            builder = builder.header(name, value);
        }
    }

    // 流式传输响应体
    let stream = response.bytes_stream();
    let body = Body::from_stream(stream);

    builder
        .body(body)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}
