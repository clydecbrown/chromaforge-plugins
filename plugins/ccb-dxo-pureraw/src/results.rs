//! PureRAW's results list (`dxo-imported-files-PureRawv3`), as observed:
//!
//! ```text
//! DxO PureRAW 3                                    the collection set's name
//! END_OF_COLLECTION_SET_SECTION
//! 2026-10-24 10:13END_OF_COLLECTION_NAME_SECTION   the collection's name; the marker ends the line
//! + selectCollection                               parameters: "+ <name>"
//! - <original A>                                   the photos processed, sorted by path
//! - <original B>
//! 0 <output of original A>                         "<index> <output>": an index into the "-" list
//! 1 <output of original B>
//! ```
//!
//! Paths are written as they are, without quoting, so nothing is trimmed from them. An index points
//! into the most recent run of `-` lines, which also reads PureRAW 2's list (pairs first, then the
//! same block). Everything is parsed in [`parse`], so other versions widen one function.

use std::collections::BTreeSet;

/// Ends the header: the collection set's and collection's names.
const HEADER_END: &str = "END_OF_COLLECTION_NAME_SECTION";

/// An output and the original it was made from, borrowed from the list: however long a line is, it
/// is never copied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pair<'a> {
    pub original: &'a str,
    pub output: &'a str,
}

/// What a results list says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Results<'a> {
    /// Each output once, in the order listed, with its original: at most the `limit` given to [`parse`].
    pub pairs: Vec<Pair<'a>>,
    /// Lines that couldn't be read, with why (1-based line numbers): at most [`MAX_PROBLEMS`].
    pub problems: Vec<String>,
    /// Lines left out past the limits (results, `-` lines and problems): counted, not kept.
    pub more: usize,
}

/// The most problems [`parse`] keeps; the rest are counted in [`Results::more`].
pub const MAX_PROBLEMS: usize = 50;

impl Results<'_> {
    fn problem(&mut self, text: impl FnOnce() -> String) {
        if self.problems.len() < MAX_PROBLEMS {
            self.problems.push(text());
        } else {
            self.more = self.more.saturating_add(1);
        }
    }
}

fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l))
}

/// Reads a results list. Lines it can't read are skipped and listed in `problems`; nothing here fails.
/// Whatever the list holds, it keeps at most `limit` results and `limit` originals per run of `-` lines,
/// so a hostile list can't run the call out of memory or time.
pub fn parse(text: &str, limit: usize) -> Results<'_> {
    let mut out = Results::default();
    // the header ends at its marker; without one (PureRAW 2), at the first "-" or "+" line
    let start = match lines(text).position(|l| l.ends_with(HEADER_END)) {
        Some(i) => i.saturating_add(1),
        None => lines(text).position(|l| l.starts_with("- ") || l.starts_with("+ ")).unwrap_or(usize::MAX),
    };
    let mut originals: Vec<&str> = Vec::new();
    let mut in_run = false;
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for (n, line) in lines(text).enumerate().skip(start) {
        let n = n.saturating_add(1);
        if line.is_empty() || line.starts_with("+ ") {
            in_run = false;
            continue;
        }
        if let Some(original) = line.strip_prefix("- ") {
            if !in_run {
                originals.clear();
                in_run = true;
            }
            if originals.len() < limit {
                originals.push(original);
            } else {
                out.more = out.more.saturating_add(1);
            }
            continue;
        }
        in_run = false;
        let Some((index, output)) = line.split_once(' ') else {
            out.problem(|| format!("line {n} isn't a result line"));
            continue;
        };
        if index.is_empty() || !index.bytes().all(|b| b.is_ascii_digit()) {
            out.problem(|| format!("line {n} isn't a result line"));
            continue;
        }
        let Some(original) = index.parse::<usize>().ok().and_then(|i| originals.get(i)) else {
            out.problem(|| format!("line {n}: {output} points at photo {index}, which the list doesn't have"));
            continue;
        };
        if output.is_empty() {
            out.problem(|| format!("line {n} names no output"));
            continue;
        }
        if seen.contains(output) {
            continue;
        }
        if out.pairs.len() >= limit {
            out.more = out.more.saturating_add(1);
            continue;
        }
        seen.insert(output);
        out.pairs.push(Pair { original, output });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair<'a>(original: &'a str, output: &'a str) -> Pair<'a> {
        Pair { original, output }
    }

    const HEADER: &str = "DxO PureRAW 3\nEND_OF_COLLECTION_SET_SECTION\n2026-10-24 10:13END_OF_COLLECTION_NAME_SECTION\n+ selectCollection\n";

    #[test]
    fn each_output_is_paired_with_the_original_its_index_points_at() {
        let text = format!("{HEADER}- /p/a/1.dng\n- /p/b/2.dng\n1 /p/b/DxO/2.dng\n0 /p/a/DxO/1.dng\n");
        let r = parse(&text, 100);
        assert_eq!(r.pairs, [pair("/p/b/2.dng", "/p/b/DxO/2.dng"), pair("/p/a/1.dng", "/p/a/DxO/1.dng")]);
        assert!(r.problems.is_empty(), "{:?}", r.problems);
    }

    #[test]
    fn an_output_listed_twice_is_kept_once() {
        let text = format!("{HEADER}- /p/a.dng\n0 /p/DxO/a.dng\n0 /p/DxO/a.dng\n");
        assert_eq!(parse(&text, 100).pairs, [pair("/p/a.dng", "/p/DxO/a.dng")]);
    }

    #[test]
    fn paths_keep_their_spaces_and_odd_characters() {
        let odd = "/p/dir with spaces/é ü/ lead and trail .dng";
        let text = format!("{HEADER}- {odd}\n0 /p/DxO/ \"q\" $;&#% .dng\n");
        assert_eq!(parse(&text, 100).pairs, [pair(odd, "/p/DxO/ \"q\" $;&#% .dng")]);
    }

    #[test]
    fn a_header_that_looks_like_a_list_line_is_still_the_header() {
        let text = "- not a photo\nEND_OF_COLLECTION_SET_SECTION\n- also notEND_OF_COLLECTION_NAME_SECTION\n- /p/a.dng\n0 /p/DxO/a.dng\n";
        assert_eq!(parse(text, 100).pairs, [pair("/p/a.dng", "/p/DxO/a.dng")]);
    }

    #[test]
    fn pureraw_2s_pairs_and_final_block_give_each_mapping_once() {
        let text = "2026-10-24 15:50\n- /p/b/1.dng\n0 /p/b/DxO/1.dng\n- /p/a/2.dng\n0 /p/a/DxO/2.dng\n+ selectCollection\n\
                    - /p/a/2.dng\n- /p/b/1.dng\n0 /p/a/DxO/2.dng\n1 /p/b/DxO/1.dng\n";
        let r = parse(text, 100);
        assert_eq!(r.pairs, [pair("/p/b/1.dng", "/p/b/DxO/1.dng"), pair("/p/a/2.dng", "/p/a/DxO/2.dng")]);
        assert!(r.problems.is_empty(), "{:?}", r.problems);
    }

    #[test]
    fn crlf_line_ends_are_read() {
        let text = format!("{}- /p/a.dng\r\n0 /p/DxO/a.dng\r\n", HEADER.replace('\n', "\r\n"));
        assert_eq!(parse(&text, 100).pairs, [pair("/p/a.dng", "/p/DxO/a.dng")]);
    }

    #[test]
    fn malformed_lines_are_skipped_and_named() {
        let text = format!(
            "{HEADER}- /p/a.dng\n0 /p/DxO/a.dng\n7 /p/DxO/x.dng\n99999999999999999999999999 /p/DxO/y.dng\n-1 /p/DxO/z.dng\nnonsense\n0\n0 \nx /p/q.dng\n"
        );
        let r = parse(&text, 100);
        assert_eq!(r.pairs, [pair("/p/a.dng", "/p/DxO/a.dng")]);
        assert_eq!(r.problems.len(), 7, "{:?}", r.problems);
        assert!(r.problems[0].contains("line 7") && r.problems[0].contains("photo 7"), "{:?}", r.problems);
    }

    #[test]
    fn an_index_before_any_list_line_points_at_nothing() {
        let text = format!("{HEADER}0 /p/DxO/a.dng\n");
        let r = parse(&text, 100);
        assert!(r.pairs.is_empty());
        assert_eq!(r.problems.len(), 1);
    }

    #[test]
    fn empty_or_garbage_input_gives_nothing_and_never_panics() {
        for text in ["", "\n", "\r\n", "END_OF_COLLECTION_NAME_SECTION", "- ", "0 ", "- \n0 ", "\u{0}\u{1}é", "+ \n- /a\n+ \n0 /b"] {
            let r = parse(text, 100);
            assert!(r.pairs.len() <= 1, "{text:?}: {r:?}");
        }
    }

    #[test]
    fn a_flood_keeps_at_most_the_limit_and_counts_the_rest() {
        let mut text = HEADER.to_string();
        for i in 0..10 {
            text.push_str(&format!("- /p/{i}.dng\n"));
        }
        for i in 0..10 {
            text.push_str(&format!("0 /p/DxO/{i}.dng\n"));
        }
        for _ in 0..(MAX_PROBLEMS + 5) {
            text.push_str("junk\n");
        }
        let r = parse(&text, 3);
        assert_eq!(r.pairs.len(), 3);
        assert_eq!(r.problems.len(), MAX_PROBLEMS);
        assert_eq!(r.more, 7 + 7 + 5, "originals, results and problems past the limits");
    }

    #[test]
    fn a_long_original_is_borrowed_not_copied_per_result() {
        let long = format!("/{}", "d".repeat(1 << 20));
        let mut text = format!("{HEADER}- {long}\n");
        for i in 0..100 {
            text.push_str(&format!("0 /o/{i}\n"));
        }
        let r = parse(&text, 100);
        assert_eq!(r.pairs.len(), 100);
        let range = text.as_ptr() as usize..text.as_ptr() as usize + text.len();
        assert!(r.pairs.iter().all(|p| range.contains(&(p.original.as_ptr() as usize))), "every original points into the list itself");
    }
}
