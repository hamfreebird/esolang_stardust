//! VM 运行时配置
//!
//! 支持在 .sd 源码顶部使用 `#[key=value]` 语法设置运行时参数。
//! 注解在词法分析前被提取并移除。
//!
//! # 支持的注解
//!
//! ```text
//! #[max_steps=5000000]
//! #[max_stack=524288]
//! #[max_heap=262144]
//! #[max_call_depth=128]
//! #[time_limit=30]
//! #[char_mode=unicode]
//! ```
//!
//! 未指定时使用合理的默认值。

/// VM 运行时可配置参数
#[derive(Debug, Clone)]
pub struct VmConfig {
    /// 最大执行步数，None 表示无限制
    pub max_steps: Option<u64>,
    /// 最大栈大小（每个帧的元素个数）
    pub max_stack_size: usize,
    /// 最大堆条目数
    pub max_heap_entries: usize,
    /// 最大调用深度
    pub max_call_depth: usize,
    /// 时间限制（秒），None 表示无限制
    pub time_limit_secs: Option<u64>,
    /// 字符输出模式
    pub char_mode: CharMode,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CharMode {
    /// 仅输出 0-127，无效码点报错
    Ascii,
    /// 支持 Unicode 码点
    Unicode,
}

impl Default for VmConfig {
    fn default() -> Self {
        VmConfig {
            max_steps: Some(10_000_000),
            max_stack_size: 1_048_576,
            max_heap_entries: 1_048_576,
            max_call_depth: 256,
            time_limit_secs: None,
            char_mode: CharMode::Ascii,
        }
    }
}

/// 从源码中提取 `#[key=value]` 注解，返回 (配置, 清理后的源码)
///
/// 注解行被整体移除，不影响后续的词法分析。
pub fn parse_config(source: &str) -> Result<(VmConfig, String), ParseConfigError> {
    let mut config = VmConfig::default();
    let mut clean = String::with_capacity(source.len());

    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(kv) = trimmed.strip_prefix("#[") {
            if let Some(kv) = kv.strip_suffix(']') {
                if let Some((key, value)) = kv.split_once('=') {
                    let key = key.trim();
                    let value = value.trim();
                    apply_config(&mut config, key, value)?;
                    continue; // 不保留注解行
                }
            }
        }
        // 保留非注解行
        if !clean.is_empty() {
            clean.push('\n');
        }
        clean.push_str(line);
    }

    Ok((config, clean))
}

fn apply_config(config: &mut VmConfig, key: &str, value: &str) -> Result<(), ParseConfigError> {
    match key {
        "max_steps" => {
            let v: u64 = value.parse().map_err(|_| ParseConfigError::InvalidValue {
                key: key.to_string(),
                value: value.to_string(),
            })?;
            config.max_steps = if v == 0 { None } else { Some(v) };
        }
        "max_stack" => {
            config.max_stack_size = value.parse().map_err(|_| ParseConfigError::InvalidValue {
                key: key.to_string(),
                value: value.to_string(),
            })?;
        }
        "max_heap" => {
            config.max_heap_entries = value.parse().map_err(|_| ParseConfigError::InvalidValue {
                key: key.to_string(),
                value: value.to_string(),
            })?;
        }
        "max_call_depth" => {
            config.max_call_depth = value.parse().map_err(|_| ParseConfigError::InvalidValue {
                key: key.to_string(),
                value: value.to_string(),
            })?;
        }
        "time_limit" => {
            let v: u64 = value.parse().map_err(|_| ParseConfigError::InvalidValue {
                key: key.to_string(),
                value: value.to_string(),
            })?;
            config.time_limit_secs = if v == 0 { None } else { Some(v) };
        }
        "char_mode" => match value {
            "ascii" => config.char_mode = CharMode::Ascii,
            "unicode" => config.char_mode = CharMode::Unicode,
            _ => {
                return Err(ParseConfigError::InvalidValue {
                    key: key.to_string(),
                    value: value.to_string(),
                });
            }
        },
        _ => {
            return Err(ParseConfigError::UnknownKey {
                key: key.to_string(),
            });
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub enum ParseConfigError {
    UnknownKey { key: String },
    InvalidValue { key: String, value: String },
}

impl std::fmt::Display for ParseConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ParseConfigError::UnknownKey { key } => write!(f, "unknown config key: '{}'", key),
            ParseConfigError::InvalidValue { key, value } => {
                write!(f, "invalid value for '{}': '{}'", key, value)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_reasonable_values() {
        let c = VmConfig::default();
        assert_eq!(c.max_steps, Some(10_000_000));
        assert_eq!(c.max_stack_size, 1_048_576);
        assert_eq!(c.max_call_depth, 256);
        assert_eq!(c.char_mode, CharMode::Ascii);
    }

    #[test]
    fn max_steps_zero_means_unlimited() {
        let mut c = VmConfig::default();
        apply_config(&mut c, "max_steps", "0").unwrap();
        assert_eq!(c.max_steps, None);
    }

    #[test]
    fn time_limit_zero_means_unlimited() {
        let mut c = VmConfig::default();
        apply_config(&mut c, "time_limit", "0").unwrap();
        assert_eq!(c.time_limit_secs, None);
    }

    #[test]
    fn parse_single_config() {
        let (c, cleaned) = parse_config("#[max_steps=500]\n+*").unwrap();
        assert_eq!(c.max_steps, Some(500));
        assert_eq!(cleaned, "+*");
    }

    #[test]
    fn parse_multiple_configs() {
        let src = "#[max_steps=100]\n#[max_stack=5000]\n#[char_mode=unicode]\n+";
        let (c, cleaned) = parse_config(src).unwrap();
        assert_eq!(c.max_steps, Some(100));
        assert_eq!(c.max_stack_size, 5000);
        assert_eq!(c.char_mode, CharMode::Unicode);
        assert_eq!(cleaned, "+");
    }

    #[test]
    fn no_config_preserves_source() {
        let src = "     +   ,\n*";
        let (_, cleaned) = parse_config(src).unwrap();
        assert_eq!(cleaned, src);
    }

    #[test]
    fn unknown_key_errors() {
        let r = parse_config("#[foo=bar]\n+");
        assert!(r.is_err());
    }

    #[test]
    fn config_lines_removed_from_middle() {
        let src = "+*\n#[max_steps=5]\n,";
        let (c, cleaned) = parse_config(src).unwrap();
        assert_eq!(c.max_steps, Some(5));
        assert_eq!(cleaned, "+*\n,");
    }
}
