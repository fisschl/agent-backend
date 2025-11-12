use anyhow::Result;
use axum::{
    extract::{Query, State, ws::WebSocketUpgrade},
    response::IntoResponse,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::Deserialize;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, http::HeaderValue, protocol::Message as WsMessage},
};
use url::Url;

use crate::AppState;

/// TTS 实时接口查询参数
#[derive(Debug, Deserialize)]
pub struct TtsRealtimeQuery {
    pub voice: String,
}

/// TTS 实时语音合成接口处理器
pub async fn handle_tts_realtime(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(query): Query<TtsRealtimeQuery>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        if let Err(e) = proxy_tts_realtime(socket, query, state.api_key).await {
            tracing::error!("TTS 实时语音合成 WebSocket 错误: {}", e);
        }
    })
}

/// 处理 TTS 实时语音合成 WebSocket 代理逻辑
async fn proxy_tts_realtime(
    client_socket: axum::extract::ws::WebSocket,
    query: TtsRealtimeQuery,
    api_key: String,
) -> Result<()> {
    // 构建目标 WSS URL，使用 Url 来管理查询参数
    let mut url = Url::parse("wss://dashscope.aliyuncs.com/api-ws/v1/realtime")?;
    url.query_pairs_mut()
        .append_pair("model", "qwen3-tts-flash-realtime")
        .append_pair("voice", &query.voice);

    // 创建 WebSocket 请求并添加 Authorization 头
    let mut request = url.as_str().into_client_request()?;

    // 设置 Authorization 头
    let auth_value = format!("Bearer {}", api_key);
    request
        .headers_mut()
        .insert("Authorization", HeaderValue::from_str(&auth_value)?);

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
                    // 客户端到上游的 Close 消息不携带载荷
                    if let Err(e) = upstream_write.send(WsMessage::Close(None)).await {
                        tracing::error!("发送 Close 到上游失败: {}", e);
                    }
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
                Ok(WsMessage::Close(close_frame)) => {
                    let close_msg = close_frame.map(|f| axum::extract::ws::CloseFrame {
                        code: f.code.into(),
                        reason: f.reason.as_ref().into(),
                    });
                    if let Err(e) = client_write
                        .send(axum::extract::ws::Message::Close(close_msg))
                        .await
                    {
                        tracing::error!("发送 Close 到客户端失败: {}", e);
                    }
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
