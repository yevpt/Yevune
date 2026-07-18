use contract::{LyricLine, StructuredLyrics};
use lofty::prelude::ItemKey;
use lofty::tag::Tag;

const MAX_LYRICS_TEXT_BYTES: usize = 256 * 1024;

pub(crate) fn from_tag(tag: &Tag) -> Option<StructuredLyrics> {
    let (text, guaranteed_unsynced) = tag
        .get_string(ItemKey::Lyrics)
        .map(|value| (value, false))
        .or_else(|| {
            tag.get_string(ItemKey::UnsyncLyrics)
                .map(|value| (value, true))
        })?;
    if text.len() > MAX_LYRICS_TEXT_BYTES {
        return None;
    }
    let mut lyrics = parse_text(text, guaranteed_unsynced);
    lyrics.lang = tag.get_string(ItemKey::Language).map(str::to_owned);
    (!lyrics.lines.is_empty()).then_some(lyrics)
}

pub(crate) fn parse_text(text: &str, guaranteed_unsynced: bool) -> StructuredLyrics {
    if guaranteed_unsynced {
        return unsynced(text);
    }

    let mut offset = 0_i64;
    let mut timed = Vec::new();
    for (sequence, raw_line) in text.lines().enumerate() {
        let line = raw_line.trim_end_matches('\r');
        if let Some(value) = metadata_value(line, "offset") {
            if let Ok(parsed) = value.parse::<i64>() {
                offset = parsed;
            }
            continue;
        }

        let mut remainder = line;
        let mut starts = Vec::new();
        while let Some(after_open) = remainder.strip_prefix('[') {
            let Some(close) = after_open.find(']') else {
                break;
            };
            let token = &after_open[..close];
            let Some(start) = parse_timestamp(token) else {
                break;
            };
            starts.push(start);
            remainder = &after_open[close + 1..];
        }
        for start in starts {
            timed.push((start, sequence, remainder.to_owned()));
        }
    }

    if timed.is_empty() {
        return unsynced(text);
    }
    timed.sort_by_key(|(start, sequence, _)| (*start, *sequence));
    StructuredLyrics {
        display_artist: None,
        display_title: None,
        lang: None,
        offset,
        synced: true,
        lines: timed
            .into_iter()
            .map(|(start, _, value)| LyricLine {
                start: Some(start),
                value,
            })
            .collect(),
    }
}

fn unsynced(text: &str) -> StructuredLyrics {
    let raw_lines = text
        .lines()
        .map(|line| line.trim_end_matches('\r'))
        .collect::<Vec<_>>();
    let first = raw_lines
        .iter()
        .position(|line| !line.is_empty())
        .unwrap_or(0);
    let end = raw_lines
        .iter()
        .rposition(|line| !line.is_empty())
        .map(|index| index + 1)
        .unwrap_or(first);
    StructuredLyrics {
        display_artist: None,
        display_title: None,
        lang: None,
        offset: 0,
        synced: false,
        lines: raw_lines[first..end]
            .iter()
            .map(|value| LyricLine {
                start: None,
                value: (*value).to_owned(),
            })
            .collect(),
    }
}

fn metadata_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let inner = line.strip_prefix('[')?.strip_suffix(']')?;
    let (candidate, value) = inner.split_once(':')?;
    candidate.eq_ignore_ascii_case(key).then_some(value.trim())
}

fn parse_timestamp(value: &str) -> Option<u64> {
    let (minutes, seconds) = value.split_once(':')?;
    let minutes = minutes.parse::<u64>().ok()?;
    let (whole_seconds, fraction) = seconds
        .split_once('.')
        .map_or((seconds, None), |(whole, fraction)| (whole, Some(fraction)));
    let whole_seconds = whole_seconds.parse::<u64>().ok()?;
    if whole_seconds >= 60 {
        return None;
    }
    let millis = match fraction {
        None | Some("") => 0,
        Some(value) if value.bytes().all(|byte| byte.is_ascii_digit()) => match value.len() {
            1 => value.parse::<u64>().ok()? * 100,
            2 => value.parse::<u64>().ok()? * 10,
            _ => value[..3].parse::<u64>().ok()?,
        },
        Some(_) => return None,
    };
    minutes
        .checked_mul(60_000)?
        .checked_add(whole_seconds * 1_000)?
        .checked_add(millis)
}

#[cfg(test)]
mod tests {
    use lofty::prelude::ItemKey;
    use lofty::tag::{Tag, TagType};

    use super::{from_tag, parse_text, MAX_LYRICS_TEXT_BYTES};

    #[test]
    fn parses_lrc_timestamps_offset_and_multiple_timestamps() {
        let parsed = parse_text(
            "[ar:Artist]\n[offset:-120]\n[00:01.25][00:02.500]First\n[01:03]Later",
            false,
        );

        assert!(parsed.synced);
        assert_eq!(parsed.offset, -120);
        assert_eq!(
            parsed
                .lines
                .iter()
                .map(|line| line.start)
                .collect::<Vec<_>>(),
            vec![Some(1_250), Some(2_500), Some(63_000)]
        );
        assert_eq!(parsed.lines[0].value, "First");
        assert_eq!(parsed.lines[1].value, "First");
    }

    #[test]
    fn falls_back_to_unsynced_lines_and_drops_blank_edges() {
        let parsed = parse_text("\nVerse one\n\nVerse two\n", false);

        assert!(!parsed.synced);
        assert_eq!(parsed.offset, 0);
        assert_eq!(
            parsed
                .lines
                .iter()
                .map(|line| line.value.as_str())
                .collect::<Vec<_>>(),
            vec!["Verse one", "", "Verse two"]
        );
        assert!(parsed.lines.iter().all(|line| line.start.is_none()));
    }

    #[test]
    fn guaranteed_unsynced_does_not_interpret_lrc_markup() {
        let parsed = parse_text("[00:01.00]Literal", true);

        assert!(!parsed.synced);
        assert_eq!(parsed.lines[0].value, "[00:01.00]Literal");
    }

    #[test]
    fn ignores_oversized_lyrics_tag() {
        let mut tag = Tag::new(TagType::Id3v2);
        tag.insert_text(ItemKey::Lyrics, "x".repeat(MAX_LYRICS_TEXT_BYTES + 1));

        assert!(from_tag(&tag).is_none());
    }
}
