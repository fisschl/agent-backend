use axum::{Router, routing::post};
use reqwest::Client;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::Level;
use tracing_subscriber::fmt::time::LocalTime;

mod handlers;

/// 应用状态
#[derive(Clone)]
pub struct AppState {
    pub http_client: Client,
    pub api_key: String,
}

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

    // 从环境变量读取 API 密钥，如果不存在则退出
    let api_key = std::env::var("DEEPSEEK_API_KEY")
        .expect("未找到 DEEPSEEK_API_KEY 环境变量，请在 .env 文件中设置或通过环境变量传入");

    // 创建应用状态
    let state = AppState {
        http_client: Client::new(),
        api_key,
    };

    // 创建路由
    let app = Router::new()
        .route(
            "/chat/completions",
            post(handlers::chat_completions::handle_chat_completions),
        )
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    // 绑定地址
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    println!("🚀 服务器启动在 http://localhost:3000");

    // 启动服务器
    axum::serve(listener, app).await.unwrap();
}
