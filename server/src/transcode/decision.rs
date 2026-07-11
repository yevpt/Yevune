//! 是否需要转码的纯判定逻辑。

use super::{TranscodeTarget, TranscodeTrack};

/// 判断曲目是否需要转换为目标格式/码率。
///
/// `raw` 始终透传；格式一致且原码率不超过目标上限时也透传。
pub fn should_transcode(track: &TranscodeTrack, target: &TranscodeTarget) -> bool {
    if target.format == "raw" {
        return false;
    }
    if !track.codec.eq_ignore_ascii_case(&target.format) {
        return true;
    }
    target.bitrate > 0 && (track.bitrate == 0 || track.bitrate > target.bitrate)
}
