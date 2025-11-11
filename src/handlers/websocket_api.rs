use anyhow::Result;
use axum::{
    extract::{Path, RawQuery, State, ws::WebSocketUpgrade},
    response::IntoResponse,
};
use futures::{sink::SinkExt, stream::StreamExt};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, http::HeaderValue, protocol::Message as WsMessage},
};

use crate::AppState;

/// WebSocket API 代理处理器
pub async fn handle_websocket_api(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(path): Path<String>,
    RawQuery(query): RawQuery,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        if let Err(e) = proxy_websocket(socket, path, query, state.api_key).await {
            tracing::error!("WebSocket 代理错误: {}", e);
        }
    })
}

/// 处理 WebSocket 代理逻辑
async fn proxy_websocket(
    client_socket: axum::extract::ws::WebSocket,
    path: String,
    query: Option<String>,
    api_key: Option<String>,
) -> Result<()> {
    // 构建目标 WSS URL
    let mut target_url = format!("wss://dashscope.aliyuncs.com/api-ws/v1/{}", path);

    // 添加查询参数
    if let Some(query_string) = query {
        target_url.push('?');
        target_url.push_str(&query_string);
    }

    // 创建 WebSocket 请求并添加 Authorization 头
    let mut request = target_url.into_client_request()?;

    // 使用从 AppState 传入的 API 密钥设置 Authorization 头
    if let Some(key) = api_key {
        let auth_value = format!("Bearer {}", key);
        request
            .headers_mut()
            .insert("Authorization", HeaderValue::from_str(&auth_value)?);
    }

    // 连接到上游 WebSocket
    let (upstream_ws, _) = connect_async(request).await?;
    let (mut upstream_write, mut upstream_read) = upstream_ws.split();

    // 分离客户端 socket
    let (mut client_write, mut client_read) = client_socket.split();

    // 客户端 -> 上游
    let client_to_upstream = async move {
        while let Some(msg) = client_read.next().await {
            match msg {
                Ok(axum::extract::ws::Message::Text(text)) => {
                    if let Err(e) = upstream_write.send(WsMessage::Text(text.to_string())).await {
                        tracing::error!("发送文本消息到上游失败: {}", e);
                        break;
                    }
                }
                Ok(axum::extract::ws::Message::Binary(data)) => {
                    if let Err(e) = upstream_write.send(WsMessage::Binary(data.to_vec())).await {
                        tracing::error!("发送二进制消息到上游失败: {}", e);
                        break;
                    }
                }
                Ok(axum::extract::ws::Message::Ping(data)) => {
                    if let Err(e) = upstream_write.send(WsMessage::Ping(data.to_vec())).await {
                        tracing::error!("发送 Ping 到上游失败: {}", e);
                        break;
                    }
                }
                Ok(axum::extract::ws::Message::Pong(data)) => {
                    if let Err(e) = upstream_write.send(WsMessage::Pong(data.to_vec())).await {
                        tracing::error!("发送 Pong 到上游失败: {}", e);
                        break;
                    }
                }
                Ok(axum::extract::ws::Message::Close(_)) => {
                    let _ = upstream_write.send(WsMessage::Close(None)).await;
                    break;
                }
                Err(e) => {
                    tracing::error!("接收客户端消息错误: {}", e);
                    break;
                }
            }
        }
    };

    // 上游 -> 客户端
    let upstream_to_client = async move {
        while let Some(msg) = upstream_read.next().await {
            match msg {
                Ok(WsMessage::Text(text)) => {
                    if let Err(e) = client_write
                        .send(axum::extract::ws::Message::Text(text.into()))
                        .await
                    {
                        tracing::error!("发送文本消息到客户端失败: {}", e);
                        break;
                    }
                }
                Ok(WsMessage::Binary(data)) => {
                    if let Err(e) = client_write
                        .send(axum::extract::ws::Message::Binary(data.into()))
                        .await
                    {
                        tracing::error!("发送二进制消息到客户端失败: {}", e);
                        break;
                    }
                }
                Ok(WsMessage::Ping(data)) => {
                    if let Err(e) = client_write
                        .send(axum::extract::ws::Message::Ping(data.into()))
                        .await
                    {
                        tracing::error!("发送 Ping 到客户端失败: {}", e);
                        break;
                    }
                }
                Ok(WsMessage::Pong(data)) => {
                    if let Err(e) = client_write
                        .send(axum::extract::ws::Message::Pong(data.into()))
                        .await
                    {
                        tracing::error!("发送 Pong 到客户端失败: {}", e);
                        break;
                    }
                }
                Ok(WsMessage::Close(_)) => {
                    let _ = client_write
                        .send(axum::extract::ws::Message::Close(None))
                        .await;
                    break;
                }
                Ok(WsMessage::Frame(_)) => {
                    // 忽略原始帧
                }
                Err(e) => {
                    tracing::error!("接收上游消息错误: {}", e);
                    break;
                }
            }
        }
    };

    // 并发处理双向消息
    tokio::select! {
        _ = client_to_upstream => {},
        _ = upstream_to_client => {},
    }

    Ok(())
}
