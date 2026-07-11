//! 图片压缩模块
//! 检测请求中的 base64 图片，超出阈值的自动压缩缩。

use image::GenericImageView;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 内存缓存条目
struct CacheEntry {
    compressed: String,
    created: Instant,
}

/// 图片压缩缓存（以 base64 前 64 字节 hash 作为 key）
static CACHE: std::sync::LazyLock<Mutex<HashMap<String, CacheEntry>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));
const CACHE_TTL: Duration = Duration::from_secs(300); // 5 分钟
const CACHE_MAX_ENTRIES: usize = 50; // 最多缓存 50 张

/// 淘汰过期条目（持有锁时调用）
fn evict_expired(cache: &mut HashMap<String, CacheEntry>) {
    if cache.len() > CACHE_MAX_ENTRIES {
        cache.retain(|_, entry| entry.created.elapsed() < CACHE_TTL);
    }
}

/// 压缩配置
#[derive(Debug, Clone)]
pub struct ImageCompressConfig {
    /// 是否启用自动压缩
    pub enabled: bool,
    /// 超过多大触发压缩（KB）
    pub max_size_kb: u64,
    /// 压缩后最长边（px）
    pub max_dimension: u32,
}

impl Default for ImageCompressConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_size_kb: 100,
            max_dimension: 1024,
        }
    }
}

/// 压缩结果
#[derive(Debug, Clone)]
pub struct CompressResult {
    /// 压缩前的图片数量
    pub images_before: u64,
    /// 压缩后的图片数量
    pub images_after: u64,
}

/// 压缩 base64 data URL 中的图片。
/// 返回 (压缩后的 data URL, px宽度, px高度)。
pub fn compress_data_url(
    data_url: &str,
    config: &ImageCompressConfig,
) -> Option<(String, u32, u32)> {
    if !config.enabled {
        return None;
    }

    // 检查是否需要压缩
    let (prefix, base64_data) = split_data_url(data_url)?;
    let raw_size = (base64_data.len() as u64) * 3 / 4 / 1024;
    if raw_size <= config.max_size_kb {
        return None; // 没超阈值，不需要压缩
    }

    // 检查缓存
    let cache_key = simple_hash(base64_data);
    {
        let cache = CACHE.lock().ok()?;
        if let Some(entry) = cache.get(&cache_key) {
            if entry.created.elapsed() < CACHE_TTL {
                return Some((
                    entry.compressed.clone(),
                    config.max_dimension,
                    config.max_dimension,
                ));
            }
        }
    }

    // decode base64
    let raw_bytes = base64_decode(base64_data)?;

    // decode image
    let img = image::load_from_memory(&raw_bytes).ok()?;
    let (orig_w, orig_h) = img.dimensions();

    // 如果已经比目标小，不压缩
    if orig_w <= config.max_dimension && orig_h <= config.max_dimension {
        return None;
    }

    // resize（保持比例）
    let (new_w, new_h) = if orig_w > orig_h {
        let ratio = config.max_dimension as f64 / orig_w as f64;
        (config.max_dimension, (orig_h as f64 * ratio).round() as u32)
    } else {
        let ratio = config.max_dimension as f64 / orig_h as f64;
        ((orig_w as f64 * ratio).round() as u32, config.max_dimension)
    };

    let resized = img.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3);

    // encode as JPEG quality 80
    let mut buf = std::io::Cursor::new(Vec::new());
    resized.write_to(&mut buf, image::ImageFormat::Jpeg).ok()?;
    let jpeg_bytes = buf.into_inner();

    let compressed_base64 = base64_encode(&jpeg_bytes);
    let new_data_url = format!("{}base64,{}", prefix, compressed_base64);

    // 存入缓存
    {
        let mut cache = CACHE.lock().ok()?;
        evict_expired(&mut cache);
        cache.insert(
            cache_key,
            CacheEntry {
                compressed: new_data_url.clone(),
                created: Instant::now(),
            },
        );
    }

    Some((new_data_url, new_w, new_h))
}

/// 从 JSON body 中压缩所有超大图片。
/// 原地修改 body，返回压缩统计信息。
pub fn compress_body_images(
    body: &mut serde_json::Value,
    config: &ImageCompressConfig,
) -> CompressResult {
    let mut result = CompressResult {
        images_before: 0,
        images_after: 0,
    };

    // Walk content arrays in both messages and input formats
    let walk = |value: &mut serde_json::Value, result: &mut CompressResult| {
        if let Some(arr) = value.as_array_mut() {
            for item in arr {
                if let Some(parts) = item.get_mut("content").and_then(|c| c.as_array_mut()) {
                    for part in parts {
                        if let Some(img_obj) = part.get_mut("image_url") {
                            if let Some(url) =
                                img_obj.get("url").and_then(serde_json::Value::as_str)
                            {
                                if url.starts_with("data:") {
                                    result.images_before += 1;
                                    if let Some((compressed, _w, _h)) =
                                        compress_data_url(url, config)
                                    {
                                        img_obj["url"] = serde_json::Value::String(compressed);
                                    }
                                    result.images_after += 1;
                                }
                            }
                        }
                    }
                }
                // 处理 input 格式中的图片（Responses API inner content）
                if let Some(parts) = item.as_array_mut() {
                    for part in parts {
                        if let Some(img_obj) = part.get_mut("image_url") {
                            if let Some(url) =
                                img_obj.get("url").and_then(serde_json::Value::as_str)
                            {
                                if url.starts_with("data:") {
                                    result.images_before += 1;
                                    if let Some((compressed, _w, _h)) =
                                        compress_data_url(url, config)
                                    {
                                        img_obj["url"] = serde_json::Value::String(compressed);
                                    }
                                    result.images_after += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    };

    if let Some(messages) = body.get_mut("messages") {
        walk(messages, &mut result);
    }
    if let Some(input) = body.get_mut("input") {
        walk(input, &mut result);
    }

    result
}

/// 分割 data URL 为 (prefix, base64_data)
fn split_data_url(data_url: &str) -> Option<(&str, &str)> {
    if !data_url.starts_with("data:") {
        return None;
    }
    let comma_pos = data_url.find(',')?;
    let prefix = &data_url[..=comma_pos];
    let base64 = &data_url[comma_pos + 1..];
    Some((prefix, base64))
}

/// base64 解码
fn base64_decode(data: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(data).ok()
}

/// base64 编码
fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

/// 简单 hash 函数（用于缓存 key）
fn simple_hash(data: &str) -> String {
    // 取前 64 和后 64 字符的拼接
    if data.len() <= 128 {
        return data.to_string();
    }
    let prefix = &data[..64];
    let suffix = &data[data.len() - 64..];
    format!("{}|{}", prefix, suffix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn split_data_url_parses_correctly() {
        let result = split_data_url("data:image/png;base64,abc123");
        assert!(result.is_some());
        let (prefix, b64) = result.unwrap();
        assert!(prefix.contains("data:image/png;base64,"));
        assert_eq!(b64, "abc123");
    }

    #[test]
    fn split_data_url_rejects_non_data_url() {
        assert!(split_data_url("https://example.com/image.png").is_none());
    }

    #[test]
    fn compress_body_images_finds_images_in_messages() {
        let small_base64 = "data:image/png;base64,abc";
        let mut body = json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "text", "text": "Look"},
                    {"type": "image_url", "image_url": {"url": small_base64}}
                ]}
            ]
        });
        let config = ImageCompressConfig::default();
        let result = compress_body_images(&mut body, &config);
        assert_eq!(result.images_before, 1);
        assert_eq!(result.images_after, 1);
        // 小图不会触发压缩，token_saved 为 0
    }

    #[test]
    fn compress_data_url_skips_when_disabled() {
        let mut config = ImageCompressConfig::default();
        config.enabled = false;
        assert!(compress_data_url("data:image/png;base64,abc", &config).is_none());
    }
}
