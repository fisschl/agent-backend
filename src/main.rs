use axum::{
    Router,
    body::Body,
    extract::{Extension, Path, RawQuery, Request},
    http::{
        HeaderMap, Method, StatusCode,
        header::{self, AUTHORIZATION, HeaderName, HeaderValue},
    },
    response::Response,
    routing::post,
};
use reqwest::Client;
use std::sync::Arc;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::Level;
use tracing_subscriber::fmt::time::LocalTime;

/// 请求头黑名单(需要移除的头)
const REQUEST_HEADERS_BLOCKLIST: &[HeaderName] = &[
    header::HOST,
    header::CONNECTION,
    header::TE,
    header::TRAILER,
    header::TRANSFER_ENCODING,
    header::UPGRADE,
    header::ORIGIN,
    header::REFERER,
];

/// 响应头黑名单(需要移除的头)
const RESPONSE_HEADERS_BLOCKLIST: &[HeaderName] = &[
    header::CONNECTION,
    header::TE,
    header::TRAILER,
    header::TRANSFER_ENCODING,
    header::UPGRADE,
    header::ACCESS_CONTROL_ALLOW_ORIGIN,
    header::ACCESS_CONTROL_ALLOW_METHODS,
    header::ACCESS_CONTROL_ALLOW_HEADERS,
    header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
    header::ACCESS_CONTROL_EXPOSE_HEADERS,
    header::ACCESS_CONTROL_MAX_AGE,
];

#[tokio::main]
async fn main() {
    // 加载 .env 文件
    dotenvy::dotenv().ok();

    // 初始化日志
    tracing_subscriber::fmt()
        .pretty()
        .with_timer(LocalTime::rfc_3339())
        .with_max_level(Level::DEBUG)
        .init();

    // 创建共享的HTTP客户端
    let http_client = Arc::new(Client::new());

    // 创建路由
    let app = Router::new()
        .route("/compatible-mode/v1/{*path}", post(proxy_handler))
        .layer(Extension(http_client))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    // 绑定地址
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    println!("🚀 服务器启动在 http://localhost:3000");

    // 启动服务器
    axum::serve(listener, app).await.unwrap();
}

/// 代理处理函数
async fn proxy_handler(
    Extension(client): Extension<Arc<Client>>,
    Path(path): Path<String>,
    RawQuery(query): RawQuery,
    method: Method,
    headers: HeaderMap,
    body: Request,
) -> Result<Response, (StatusCode, String)> {
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

    // 从环境变量读取API密钥并设置Authorization头(仅当未传入时)
    if !request_headers.contains_key(AUTHORIZATION) {
        if let Some(auth_value) = std::env::var("DASHSCOPE_API_KEY")
            .ok()
            .and_then(|key| HeaderValue::from_str(&format!("Bearer {}", key)).ok())
        {
            request_headers.insert(AUTHORIZATION, auth_value);
        }
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
