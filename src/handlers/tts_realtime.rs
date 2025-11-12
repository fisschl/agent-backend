use anyhow::Result;
use axum::{
    extract::{Query, State, ws::WebSocketUpgrade},
    response::IntoResponse,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use futures::{sink::SinkExt, stream::StreamExt};
use pulldown_cmark::{Event, Options, Parser, TagEnd};
use serde::Deserialize;
use serde_json::json;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, http::HeaderValue, protocol::Message as WsMessage},
};
use url::Url;
use uuid::Uuid;

use crate::AppState;

/// TTS 实时接口查询参数
#[derive(Debug, Deserialize)]
pub struct TtsRealtimeQuery {
    pub voice: String,
}

/// 将 Markdown 转换为适合语音输出的纯文本
fn markdown_to_plain_text(text: &str) -> String {
    // 启用所有常用的 Markdown 扩展功能
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES); // 表格
    options.insert(Options::ENABLE_FOOTNOTES); // 脚注
    options.insert(Options::ENABLE_STRIKETHROUGH); // 删除线 ~~text~~
    options.insert(Options::ENABLE_TASKLISTS); // 任务列表 - [ ] task
    options.insert(Options::ENABLE_SMART_PUNCTUATION); // 智能标点符号
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES); // 标题属性
    options.insert(Options::ENABLE_MATH); // 数学公式
    options.insert(Options::ENABLE_GFM); // GitHub Flavored Markdown
    options.insert(Options::ENABLE_DEFINITION_LIST); // 定义列表

    let parser = Parser::new_ext(text, options);
    let mut plain_text = String::new();

    for event in parser {
        match event {
            // 提取文本内容
            Event::Text(t) | Event::Code(t) => {
                plain_text.push_str(&t);
            }
            // 软换行保留为空格
            Event::SoftBreak => plain_text.push(' '),
            // 硬换行保留为换行符
            Event::HardBreak => plain_text.push('\n'),
            // 块级元素结束统一添加换行
            Event::End(TagEnd::Heading(_))
            | Event::End(TagEnd::Paragraph)
            | Event::End(TagEnd::Item)
            | Event::End(TagEnd::BlockQuote(_))
            | Event::End(TagEnd::CodeBlock)
            | Event::End(TagEnd::List(_))
            | Event::End(TagEnd::Table)
            | Event::End(TagEnd::TableRow)
            | Event::End(TagEnd::TableCell)
            | Event::End(TagEnd::DefinitionListTitle)
            | Event::End(TagEnd::DefinitionListDefinition) => {
                plain_text.push('\n');
            }
            // 行内元素忽略（文本之间已经有正常的空格）
            // 其他元素忽略
            _ => {}
        }
    }

    plain_text.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_to_plain_text_basic() {
        let input = "Hello **world**!";
        let output = markdown_to_plain_text(input);
        assert_eq!(output, "Hello world!");
    }

    #[test]
    fn test_markdown_to_plain_text_headers() {
        let input = "## Heading\nContent";
        let output = markdown_to_plain_text(input);
        // 标题后有换行，段落也会添加换行
        assert!(output.contains("Heading"));
        assert!(output.contains("Content"));
        // 打印实际输出以验证
        println!("Output: {:?}", output);
    }

    #[test]
    fn test_markdown_to_plain_text_paragraphs() {
        let input = "Paragraph 1\n\nParagraph 2\n\nParagraph 3";
        let output = markdown_to_plain_text(input);
        println!("Paragraphs output: {:?}", output);
        assert!(output.contains("Paragraph 1"));
        assert!(output.contains("Paragraph 2"));
        assert!(output.contains("Paragraph 3"));
    }

    #[test]
    fn test_markdown_to_plain_text_links() {
        let input = "Check [this link](https://example.com) out";
        let output = markdown_to_plain_text(input);
        assert_eq!(output, "Check this link out");
    }

    #[test]
    fn test_markdown_to_plain_text_code() {
        let input = "Use `code` here";
        let output = markdown_to_plain_text(input);
        assert_eq!(output, "Use code here");
    }

    #[test]
    fn test_markdown_to_plain_text_lists() {
        let input = "- Item 1\n- Item 2\n- Item 3";
        let output = markdown_to_plain_text(input);
        assert_eq!(output, "Item 1\nItem 2\nItem 3");
    }

    #[test]
    fn test_markdown_to_plain_text_table() {
        let input = "| Name | Age |\n|------|-----|\n| Alice| 30  |\n| Bob  | 25  |";
        let output = markdown_to_plain_text(input);
        assert!(output.contains("Name"));
        assert!(output.contains("Age"));
        assert!(output.contains("Alice"));
        assert!(output.contains("30"));
        assert!(output.contains("Bob"));
        assert!(output.contains("25"));
    }

    #[test]
    fn test_markdown_to_plain_text_strikethrough() {
        let input = "This is ~~wrong~~ correct";
        let output = markdown_to_plain_text(input);
        assert_eq!(output, "This is wrong correct");
    }

    #[test]
    fn test_markdown_to_plain_text_emphasis() {
        let input = "This is *italic* and **bold** text";
        let output = markdown_to_plain_text(input);
        assert_eq!(output, "This is italic and bold text");
    }

    #[test]
    fn test_markdown_to_plain_text_task_list() {
        let input = "- [x] Done\n- [ ] Todo";
        let output = markdown_to_plain_text(input);
        assert!(output.contains("Done"));
        assert!(output.contains("Todo"));
    }

    #[test]
    fn test_markdown_to_plain_text_blockquote() {
        let input = "> This is a quote";
        let output = markdown_to_plain_text(input);
        assert_eq!(output, "This is a quote");
    }

    #[test]
    fn test_markdown_to_plain_text_complex() {
        let input = "## Hello **World**\n\nThis is a `code` example with [link](url).\n\n- Item 1\n- Item 2";
        let output = markdown_to_plain_text(input);
        assert!(output.contains("Hello World"));
        assert!(output.contains("code"));
        assert!(output.contains("link"));
        assert!(output.contains("Item 1"));
        assert!(output.contains("Item 2"));
    }
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

    // 发送初始化消息
    let session_update = json!({
        "event_id": Uuid::now_v7().to_string(),
        "type": "session.update",
        "session": {
            "voice": query.voice,
            "response_format": "pcm",
            "sample_rate": 24000
        }
    });

    let init_message = serde_json::to_string(&session_update)?;
    upstream_write.send(WsMessage::Text(init_message)).await?;
    tracing::debug!("已发送 session.update 消息");

    // 等待 100 毫秒
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // 分离客户端 socket
    let (mut client_write, mut client_read) = client_socket.split();

    // 客户端 -> 上游
    let client_to_upstream = async move {
        while let Some(msg) = client_read.next().await {
            match msg {
                Ok(axum::extract::ws::Message::Text(text)) => {
                    // 预处理：移除 Markdown 格式字符
                    let text_str = markdown_to_plain_text(&text.to_string());
                    tracing::debug!("Markdown 转换后的文本: {}", text_str);

                    // 如果文本超过 100 字符，按空白字符切分
                    let chunks: Vec<&str> = if text_str.len() > 100 {
                        text_str.split_whitespace().collect()
                    } else {
                        vec![text_str.as_str()]
                    };

                    // 依次发送每个文本片段
                    for chunk in chunks {
                        let input_message = json!({
                            "event_id": Uuid::now_v7().to_string(),
                            "type": "input_text_buffer.append",
                            "text": chunk
                        });

                        let message_str = match serde_json::to_string(&input_message) {
                            Ok(s) => s,
                            Err(e) => {
                                tracing::error!("JSON 序列化失败: {}", e);
                                break;
                            }
                        };

                        if let Err(e) = upstream_write.send(WsMessage::Text(message_str)).await {
                            tracing::error!("发送文本消息到上游失败: {}", e);
                            break;
                        }

                        tracing::debug!("已发送文本消息到上游: {}", chunk);

                        // 等待 200 毫秒
                        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                    }

                    let commit_message = json!({
                        "event_id": Uuid::now_v7().to_string(),
                        "type": "input_text_buffer.commit"
                    });

                    let message_str = match serde_json::to_string(&commit_message) {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!("JSON 序列化失败: {}", e);
                            break;
                        }
                    };

                    if let Err(e) = upstream_write.send(WsMessage::Text(message_str)).await {
                        tracing::error!("发送 commit 消息到上游失败: {}", e);
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
                // 忽略 Ping、Pong、Binary 消息
                Ok(_) => {}
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

                    // 只处理 response.audio.delta 类型
                    if msg_type != "response.audio.delta" {
                        tracing::debug!("收到上游消息，已忽略: {}", text);
                        continue;
                    }

                    // 提取 delta 字段
                    let delta_base64 = match json_value.get("delta").and_then(|v| v.as_str()) {
                        Some(d) => d,
                        None => {
                            tracing::warn!("response.audio.delta 消息缺少 delta 字段");
                            continue;
                        }
                    };

                    // Base64 解码
                    let audio_data = match STANDARD.decode(delta_base64) {
                        Ok(data) => data,
                        Err(e) => {
                            tracing::error!("Base64 解码失败: {}", e);
                            continue;
                        }
                    };

                    // 发送音频数据到客户端
                    if let Err(e) = client_write
                        .send(axum::extract::ws::Message::Binary(audio_data.into()))
                        .await
                    {
                        tracing::error!("发送音频数据到客户端失败: {}", e);
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
                // 忽略其他消息类型
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
