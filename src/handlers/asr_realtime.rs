use anyhow::Result;
use axum::{
    extract::{State, ws::WebSocketUpgrade},
    response::IntoResponse,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use futures::{sink::SinkExt, stream::StreamExt};
use serde_json::json;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, http::HeaderValue, protocol::Message as WsMessage},
};
use url::Url;
use uuid::Uuid;

use crate::AppState;

/// ASR 实时语音识别接口处理器
pub async fn handle_asr_realtime(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        if let Err(e) = proxy_asr_realtime(socket, state.api_key).await {
            tracing::error!("ASR 实时语音识别 WebSocket 错误: {}", e);
        }
    })
}

/// 处理 ASR 实时语音识别 WebSocket 代理逻辑
async fn proxy_asr_realtime(
    client_socket: axum::extract::ws::WebSocket,
    api_key: String,
) -> Result<()> {
    // 构建目标 WSS URL
    let mut url = Url::parse("wss://dashscope.aliyuncs.com/api-ws/v1/realtime")?;
    url.query_pairs_mut()
        .append_pair("model", "qwen3-asr-flash-realtime");

    // 创建 WebSocket 请求并添加必要的请求头
    let mut request = url.as_str().into_client_request()?;

    // 设置 Authorization 头
    let auth_value = format!("Bearer {}", api_key);
    request
        .headers_mut()
        .insert("Authorization", HeaderValue::from_str(&auth_value)?);

    // 设置 OpenAI-Beta 头（API 要求）
    request
        .headers_mut()
        .insert("OpenAI-Beta", HeaderValue::from_str("realtime=v1")?);

    // 连接到上游 WebSocket
    let (upstream_ws, _) = connect_async(request).await?;
    let (mut upstream_write, mut upstream_read) = upstream_ws.split();

    // 构建 session.update 消息（启用 VAD 模式）
    let session_update = json!({
        "event_id": Uuid::now_v7().to_string(),
        "type": "session.update",
        "session": {
            "modalities": ["text"],
            "input_audio_format": "pcm",
            "sample_rate": 16000,
            "turn_detection": {
                "type": "server_vad"
            }
        }
    });

    let init_message = serde_json::to_string(&session_update)?;
    upstream_write.send(WsMessage::Text(init_message)).await?;
    tracing::debug!("已发送 session.update 消息");

    // 等待 session 配置完成
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // 分离客户端 socket
    let (mut client_write, mut client_read) = client_socket.split();

    // 客户端 -> 上游（音频数据发送）
    let client_to_upstream = async move {
        while let Some(msg) = client_read.next().await {
            match msg {
                Ok(axum::extract::ws::Message::Binary(audio_data)) => {
                    // 将音频数据编码为 Base64
                    let encoded_audio = STANDARD.encode(&audio_data);

                    // 构建 input_audio_buffer.append 消息
                    let append_message = json!({
                        "event_id": Uuid::now_v7().to_string(),
                        "type": "input_audio_buffer.append",
                        "audio": encoded_audio
                    });

                    let message_str = match serde_json::to_string(&append_message) {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!("JSON 序列化失败: {}", e);
                            break;
                        }
                    };

                    if let Err(e) = upstream_write.send(WsMessage::Text(message_str)).await {
                        tracing::error!("发送音频消息到上游失败: {}", e);
                        break;
                    }

                    tracing::debug!("已发送音频数据到上游");
                }
                Ok(axum::extract::ws::Message::Close(_)) => {
                    if let Err(e) = upstream_write.send(WsMessage::Close(None)).await {
                        tracing::error!("发送 Close 到上游失败: {}", e);
                    }
                    break;
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::error!("接收客户端消息错误: {}", e);
                    break;
                }
            }
        }
    };

    // 上游 -> 客户端（识别结果接收）
    let upstream_to_client = async move {
        while let Some(msg) = upstream_read.next().await {
            match msg {
                Ok(WsMessage::Text(text)) => {
                    // 解析 JSON 消息
                    let json_value: serde_json::Value = match serde_json::from_str(&text) {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::warn!("解析上游 JSON 消息失败: {}, 原始消息: {}", e, text);
                            continue;
                        }
                    };

                    // 提取 type 字段
                    let msg_type = json_value
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    // 处理转录相关的事件
                    match msg_type {
                        "conversation.item.input_audio_transcription.text" => {
                            // 增量转录结果，直接返回纯文本
                            let Some(text) = json_value.get("text").and_then(|v| v.as_str()) else {
                                continue;
                            };

                            if let Err(e) = client_write
                                .send(axum::extract::ws::Message::Text(text.to_string().into()))
                                .await
                            {
                                tracing::error!("发送转录文本到客户端失败: {}", e);
                                break;
                            }

                            tracing::debug!("转录文本: {}", text);
                        }
                        "conversation.item.input_audio_transcription.failed" => {
                            // 转录失败消息仅记录日志，不转发给客户端
                            tracing::error!("音频转录失败: {}", text);
                        }
                        "error" => {
                            // 错误消息仅记录日志，不转发给客户端
                            tracing::error!("上游错误: {}", text);
                        }
                        _ => {
                            // 其他消息类型输出完整消息体
                            tracing::debug!("忽略消息: {}", text);
                        }
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
                Ok(_) => {}
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
