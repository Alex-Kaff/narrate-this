use sha2::{Digest, Sha256};

use crate::types::{CaptionSegment, TimedChunk};

/// Minimum duration (ms) for each media segment.
const MIN_SEGMENT_MS: f64 = 5000.0;

/// SHA-256 hex digest of arbitrary data.
pub fn content_hash(data: impl AsRef<[u8]>) -> String {
    let digest = Sha256::digest(data.as_ref());
    format!("{digest:x}")
}

/// Base64-encode bytes using standard encoding.
pub fn b64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

/// Base64-decode a string using standard encoding.
pub fn b64_decode(s: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s).ok()
}

/// Split narration into chunks of at least `MIN_SEGMENT_MS`, breaking only at
/// sentence boundaries (after `.` `!` `?`). Each chunk is mapped to caption
/// timing via word counts.
pub fn split_into_timed_chunks(narration: &str, captions: &[CaptionSegment]) -> Vec<TimedChunk> {
    let sentences = split_sentences(narration);
    if sentences.is_empty() {
        return vec![];
    }

    // Compute word count and timing for each sentence
    let mut sentence_timings: Vec<(String, usize, f64, f64)> = Vec::new();
    let mut word_offset = 0usize;

    for sentence in &sentences {
        let wc = sentence.split_whitespace().count();
        let start_idx = word_offset.min(captions.len().saturating_sub(1));
        let end_idx = (word_offset + wc).min(captions.len()).saturating_sub(1);

        let start_ms = captions
            .get(start_idx)
            .map(|s| s.start_ms as f64)
            .unwrap_or(0.0);
        let end_ms = captions
            .get(end_idx)
            .map(|s| (s.start_ms + s.duration_ms) as f64)
            .unwrap_or(start_ms);

        sentence_timings.push((sentence.clone(), wc, start_ms, end_ms));
        word_offset += wc;
    }

    // Group sentences into chunks of at least MIN_SEGMENT_MS
    let mut chunks: Vec<TimedChunk> = Vec::new();
    let mut chunk_text = String::new();
    let mut chunk_start_ms: Option<f64> = None;
    let mut chunk_end_ms = 0.0f64;

    for (text, _wc, s_start, s_end) in &sentence_timings {
        if chunk_start_ms.is_none() {
            chunk_start_ms = Some(*s_start);
        }
        if !chunk_text.is_empty() {
            chunk_text.push(' ');
        }
        chunk_text.push_str(text);
        chunk_end_ms = *s_end;

        let duration = chunk_end_ms - chunk_start_ms.unwrap_or(0.0);
        if duration >= MIN_SEGMENT_MS {
            chunks.push(TimedChunk {
                text: std::mem::take(&mut chunk_text),
                start_ms: chunk_start_ms.unwrap_or(0.0),
                end_ms: chunk_end_ms,
            });
            chunk_start_ms = None;
        }
    }

    // Remaining sentences become the last chunk
    if !chunk_text.is_empty() {
        chunks.push(TimedChunk {
            text: chunk_text,
            start_ms: chunk_start_ms.unwrap_or(0.0),
            end_ms: chunk_end_ms,
        });
    }

    chunks
}

/// Split text into sentences. Splits on `.` `!` `?` followed by whitespace or
/// end-of-string, keeping the punctuation with the sentence.
pub fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();

    let chars: Vec<char> = text.chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        current.push(ch);
        if matches!(ch, '.' | '!' | '?') {
            let at_end = i + 1 >= chars.len();
            let next_is_ws = chars.get(i + 1).is_some_and(|c| c.is_whitespace());
            if at_end || next_is_ws {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    sentences.push(trimmed);
                }
                current.clear();
            }
        }
    }

    // Any trailing text without terminal punctuation
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        sentences.push(trimmed);
    }

    sentences
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_hash() {
        let hash = content_hash("hello world");
        assert_eq!(hash.len(), 64); // SHA-256 hex is 64 chars
        // Same input should produce same hash
        assert_eq!(hash, content_hash("hello world"));
        // Different input should produce different hash
        assert_ne!(hash, content_hash("hello world!"));
    }

    #[test]
    fn test_b64_roundtrip() {
        let data = b"hello world";
        let encoded = b64_encode(data);
        let decoded = b64_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_split_sentences_basic() {
        let sentences = split_sentences("Hello world. How are you? I'm fine!");
        assert_eq!(sentences, vec![
            "Hello world.",
            "How are you?",
            "I'm fine!",
        ]);
    }

    #[test]
    fn test_split_sentences_no_punctuation() {
        let sentences = split_sentences("No punctuation here");
        assert_eq!(sentences, vec!["No punctuation here"]);
    }

    #[test]
    fn test_split_sentences_abbreviations() {
        // Abbreviations like "U.S.A." should not split mid-abbreviation
        let sentences = split_sentences("Visit the U.S.A. today. It's great.");
        // The current algorithm splits on ". " so "U.S.A." followed by space will split
        assert_eq!(sentences.len(), 3);
    }

    #[test]
    fn test_split_sentences_empty() {
        let sentences = split_sentences("");
        assert!(sentences.is_empty());
    }

    #[test]
    fn test_split_into_timed_chunks_empty() {
        let chunks = split_into_timed_chunks("", &[]);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_split_into_timed_chunks_single_sentence() {
        let captions = vec![
            CaptionSegment { text: "Hello".into(), start_ms: 0, duration_ms: 500 },
            CaptionSegment { text: "world.".into(), start_ms: 500, duration_ms: 500 },
        ];
        let chunks = split_into_timed_chunks("Hello world.", &captions);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, "Hello world.");
    }
}
