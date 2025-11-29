use anyhow::Result;
use axum::{
    extract::{Query, State, ws::WebSocketUpgrade},
    response::IntoResponse,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use futures::{sink::SinkExt, stream::StreamExt};
use regex::Regex;
use serde::Deserialize;
use serde_json::json;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, http::HeaderValue, protocol::Message as WsMessage},
};
use unicode_normalization::UnicodeNormalization;
use url::Url;
use uuid::Uuid;

use crate::AppState;

/// TTS 实时接口查询参数
#[derive(Debug, Deserialize)]
pub struct TtsRealtimeQuery {
    pub voice: String,
}

/// 将文本清洗为适合语音输出的纯文本
///
/// 处理流程：
/// 1. 归一化行结束符（统一为 \n）
/// 2. Unicode 归一化（NFKC，统一全角/兼容字符）
/// 3. 统一空白字符为普通空格，保留换行
/// 4. 过滤特殊符号，仅保留：字母、数字、逗号、句号、换行、空白
/// 5. 压缩多余空格与空行
fn sanitize_text(text: &str) -> String {
    // 1. 归一化行结束符
    let normalized_lines = text.replace("\r\n", "\n").replace('\r', "\n");

    // 2. Unicode 归一化（NFKC）
    let normalized: String = normalized_lines.nfkc().collect();

    // 3. 统一空白字符（保留换行）
    let unified_whitespace = normalized
        .chars()
        .map(|c| match c {
            '\n' => '\n',
            c if c.is_whitespace() => ' ',
            c => c,
        })
        .collect::<String>();

    // 4. 过滤特殊符号（白名单：字母、数字、逗号、句号、换行、空白）
    // 保留：\p{L}(字母)、\p{N}(数字)、\p{Zs}(分隔空白)、,，、(逗号/顿号)、.。．(句号)、\n(换行)
    let re = Regex::new(r"[^\p{L}\p{N}\p{Zs},，、.。．\n]+").unwrap();
    let filtered = re.replace_all(&unified_whitespace, "");

    // 5. 压缩多余空格
    let re_spaces = Regex::new(r" +").unwrap();
    let compressed_spaces = re_spaces.replace_all(&filtered, " ");

    // 6. 压缩多余空行（最多保留 2 个连续换行）
    let re_newlines = Regex::new(r"\n{3,}").unwrap();
    let compressed_newlines = re_newlines.replace_all(&compressed_spaces, "\n\n");

    // 7. 清理首尾空白
    compressed_newlines.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_text_basic_symbols() {
        let input = "Hello **world**!";
        let output = sanitize_text(input);
        assert_eq!(output, "Hello world");
    }

    #[test]
    fn test_sanitize_text_markdown_headers() {
        let input = "## Heading\nContent";
        let output = sanitize_text(input);
        assert_eq!(output, "Heading\nContent");
    }

    #[test]
    fn test_sanitize_text_paragraphs() {
        let input = "Paragraph 1\n\nParagraph 2\n\nParagraph 3";
        let output = sanitize_text(input);
        assert!(output.contains("Paragraph 1"));
        assert!(output.contains("Paragraph 2"));
        assert!(output.contains("Paragraph 3"));
    }

    #[test]
    fn test_sanitize_text_links() {
        let input = "Check [this link](https://example.com) out";
        let output = sanitize_text(input);
        // 括号与冒号被过滤，URL 会紧邻前后文本
        assert_eq!(output, "Check this linkhttpsexample.com out");
    }

    #[test]
    fn test_sanitize_text_emoji_and_symbols() {
        let input = "Hello 😊 #Topic @User";
        let output = sanitize_text(input);
        assert_eq!(output, "Hello Topic User");
    }

    #[test]
    fn test_sanitize_text_chinese_punctuation() {
        let input = "示例：价格为￥99.99（约）";
        let output = sanitize_text(input);
        // 冒号、货币符号、括号被过滤
        assert_eq!(output, "示例价格为99.99约");
    }

    #[test]
    fn test_sanitize_text_preserve_common_punctuation() {
        let input = "条目A，条目B，条目C。";
        let output = sanitize_text(input);
        // NFKC 将全角逗号、句号归一化为半角（这是期望行为）
        assert_eq!(output, "条目A,条目B,条目C。");
    }

    #[test]
    fn test_sanitize_text_list_markers() {
        let input = "- Item 1\n- Item 2\n- Item 3";
        let output = sanitize_text(input);
        assert!(output.contains("Item 1"));
        assert!(output.contains("Item 2"));
        assert!(output.contains("Item 3"));
    }

    #[test]
    fn test_sanitize_text_table() {
        let input = "| Name | Age |\n|------|-----|\n| Alice| 30  |\n| Bob  | 25  |";
        let output = sanitize_text(input);
        assert!(output.contains("Name"));
        assert!(output.contains("Age"));
        assert!(output.contains("Alice"));
        assert!(output.contains("30"));
        assert!(output.contains("Bob"));
        assert!(output.contains("25"));
    }

    #[test]
    fn test_sanitize_text_multiple_spaces() {
        let input = "Hello    world    test";
        let output = sanitize_text(input);
        assert_eq!(output, "Hello world test");
    }

    #[test]
    fn test_sanitize_text_excessive_newlines() {
        let input = "Line 1\n\n\n\n\nLine 2";
        let output = sanitize_text(input);
        assert_eq!(output, "Line 1\n\nLine 2");
    }

    #[test]
    fn test_sanitize_text_windows_line_endings() {
        let input = "Line 1\r\nLine 2\r\nLine 3";
        let output = sanitize_text(input);
        assert_eq!(output, "Line 1\nLine 2\nLine 3");
    }

    #[test]
    fn test_sanitize_text_unicode_normalization() {
        // NFKC 将全角逗号归一化为半角，全角句号保持不变
        let input = "测试，全角。字符";
        let output = sanitize_text(input);
        assert_eq!(output, "测试,全角。字符");
    }

    #[test]
    fn test_sanitize_text_decimal_numbers() {
        let input = "Price: $99.99 or 1,234.56";
        let output = sanitize_text(input);
        assert_eq!(output, "Price 99.99 or 1,234.56");
    }

    #[test]
    fn test_sanitize_text_real_world_tts() {
        // 模拟实际 TTS 输入场景
        let input = "## 你好！欢迎使用 **AI 助手**\n\n这是一段包含 Markdown、符号（@#$%）和 Emoji 😊 的文本。\n\n- 列表项 1\n- 列表项 2";
        let output = sanitize_text(input);
        // 期望结果：移除所有格式符号，保留文本、空格、换行和基本标点
        assert!(output.contains("你好"));
        assert!(output.contains("欢迎使用"));
        assert!(output.contains("AI"));
        assert!(output.contains("助手"));
        assert!(!output.contains("**"));
        assert!(!output.contains("##"));
        assert!(!output.contains("@"));
        assert!(!output.contains("#"));
        assert!(!output.contains("$"));
        assert!(!output.contains("%"));
        assert!(!output.contains("😊"));
        assert!(output.contains("列表项"));
    }

    #[test]
    fn test_sanitize_text_multilingual_support() {
        // 测试多语言支持：中文、日文、韩文、阿拉伯文、俄文
        let input = "中文：你好世界！ 日本語：こんにちは！ 한국어：안녕하세요！ العربية：مرحبا！ Русский：Привет！";
        let output = sanitize_text(input);

        // 验证所有语言文字都被保留
        assert!(output.contains("你好世界"));
        assert!(output.contains("こんにちは"));
        assert!(output.contains("안녕하세요"));
        assert!(output.contains("مرحبا"));
        assert!(output.contains("Привет"));

        // 验证感叹号被过滤
        assert!(!output.contains("！"));
    }

    #[test]
    fn test_sanitize_text_unicode_categories() {
        // 测试 Unicode 属性类别的正确识别
        // \p{L} - 字母（所有语言）
        // \p{N} - 数字（所有数字系统）
        let input = "English字母123数字٣٤٥阿拉伯数字";
        let output = sanitize_text(input);

        // 所有字母和数字都应该保留（阿拉伯数字也是 \p{N}）
        assert_eq!(output, "English字母123数字٣٤٥阿拉伯数字");
    }

    #[test]
    fn test_sanitize_text_nfkc_normalization() {
        // 测试 NFKC 归一化：全角 → 半角转换
        let input = "ＨＥＬＬＯｗｏｒｌｄ１２３";
        let output = sanitize_text(input);

        // 全角拉丁字母和数字应转为半角
        assert_eq!(output, "HELLOworld123");
    }

    #[test]
    fn test_sanitize_text_cjk_punctuation() {
        // 测试中日韩标点的处理
        let input = "中文，标点。日文、句読点。韓国語、句読点。";
        let output = sanitize_text(input);

        // 逗号（、和，）及句号（。）应保留
        assert!(output.contains(",")); // 全角逗号归一化为半角
        assert!(output.contains("、")); // 顿号保留
        assert!(output.contains("。")); // 全角句号保留
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
                    // 预处理：清洗文本，移除特殊符号
                    let text_str = sanitize_text(&text.to_string());
                    tracing::debug!("文本清洗后: {}", text_str);

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
