//! QQ 音乐签名算法。对照旧版 `platforms/QQMusic/qq_sign.py`。
//!
//! 旧版是纯算法（SHA1 + base64 + 异或 + 字符串拼凑），逐行翻译。
//! - `sha1` crate 对照 `hashlib.sha1`
//! - `base64` crate 对照 `base64.b64encode`
//! - JSON 序列化用 serde_json（必须开 `preserve_order`，对照 orjson 保持插入序）

use base64::{engine::general_purpose::STANDARD, Engine};
use sha1::{Digest, Sha1};

/// 签名参数索引。对照旧版 `PART_1_INDEXES = [23,14,6,36,16,40,7,19]`，
/// 过滤 `>= 40` 后（旧版 line 19 的 `filter(lambda x: x < 40)`）剩 7 个。
const PART_1_INDEXES: [usize; 7] = [23, 14, 6, 36, 16, 7, 19];

/// 签名参数索引。对照旧版 `PART_2_INDEXES`。
const PART_2_INDEXES: [usize; 8] = [16, 1, 32, 12, 19, 27, 8, 5];

/// 异或混淆值。对照旧版 `SCRAMBLE_VALUES`（20 个）。
const SCRAMBLE_VALUES: [u8; 20] = [
    89, 39, 179, 150, 218, 82, 58, 252, 177, 52, 186, 123, 120, 64, 242, 133, 143, 161, 121, 179,
];

/// QQ 音乐请求签名。对照旧版 `sign(request)`。
///
/// 算法（逐行对照 qq_sign.py:22-52）：
/// 1. JSON 序列化（compact、保持插入序） → SHA1 哈希 → 大写 hex（40 字符）
/// 2. part1 = 取 PART_1_INDEXES 指定位置的字符拼接
/// 3. part2 = 取 PART_2_INDEXES 指定位置的字符拼接
/// 4. part3 = 20 字节：SCRAMBLE_VALUES[i] ^ hash_hex 第 i 个字节对
/// 5. b64_part = base64(part3) 去掉 `\` `/` `+` `=`
/// 6. 最终 = `"zzc" + part1 + b64_part + part2`，全部转小写
pub fn sign(request: &serde_json::Value) -> String {
    // 1. JSON compact 序列化（保持插入序，无空格，对照 orjson.dumps）。
    //    serde_json::to_string 默认 compact 且不含换行/空格，与 orjson 一致。
    let json_str = serde_json::to_string(request).expect("request 序列化不会失败");

    // 2. SHA1 → 大写 hex（对照 hashlib.sha1(json_str).hexdigest().upper()）。
    let mut hasher = Sha1::new();
    hasher.update(json_str.as_bytes());
    let hash_hex = hex_upper(&hasher.finalize());

    // 3. part1：按索引取字符（对照 part1 = "".join(hash_hex[i] for i in PART_1_INDEXES)）。
    let part1: String = PART_1_INDEXES
        .iter()
        .map(|&i| hash_hex.as_bytes()[i] as char)
        .collect();

    // 4. part2：按索引取字符。
    let part2: String = PART_2_INDEXES
        .iter()
        .map(|&i| hash_hex.as_bytes()[i] as char)
        .collect();

    // 5. part3：异或混淆（对照 for i, v in enumerate(SCRAMBLE_VALUES): value = v ^ int(hash_hex[i*2:i*2+2], 16)）。
    let mut part3 = [0u8; 20];
    for (i, &v) in SCRAMBLE_VALUES.iter().enumerate() {
        // 取 hash_hex 的第 i 个字节对（2 个 hex 字符）转成字节值
        let byte_val = u8::from_str_radix(&hash_hex[i * 2..i * 2 + 2], 16)
            .expect("hash_hex 字节对解析不会失败");
        part3[i] = v ^ byte_val;
    }

    // 6. base64 编码并移除 `\` `/` `+` `=`（对照 re.sub(rb"[\\/+=]", b"", base64.b64encode(part3))）。
    let b64 = STANDARD.encode(part3);
    let b64_part: String = b64
        .chars()
        .filter(|c| !matches!(c, '\\' | '/' | '+' | '='))
        .collect();

    // 7. 组合并转小写（对照 f"zzc{part1}{b64_part}{part2}".lower()）。
    format!("zzc{part1}{b64_part}{part2}").to_lowercase()
}

/// 把字节切片转为大写 hex 字符串（对照 hexdigest().upper()）。
fn hex_upper(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// ground truth：用旧版 Python（orjson）对同一输入算出的真实签名。
    /// 采集自 `platforms/QQMusic/qq_sign.py` 的 __main__ 测试块。
    #[test]
    fn sign_matches_python_ground_truth() {
        let request = json!({
            "comm": {"ct": "11", "cv": "13020508"},
            "music.srfDissInfo.DissInfo": {
                "disstid": "9595891286",
                "song_begin": 0,
                "song_num": 10,
            }
        });
        let expected = "zzc94de0f2x4zqstyrnrs9hmoy5x0ag8hlug068a25e9";
        assert_eq!(sign(&request), expected);
    }

}
