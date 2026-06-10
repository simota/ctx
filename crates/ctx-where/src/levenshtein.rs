// crates/ctx-where/src/levenshtein.rs
//
// Pure port of internal/where.Levenshtein — classical two-row DP, unit
// costs. Inputs are expected to be lower-cased by the caller.

/// Edit distance between two char slices. Operations: insert, delete,
/// substitute, all cost 1. Matches the Go source byte-for-byte on the
/// "iterate with shorter dim along the row" optimisation.
pub fn levenshtein(a: &[char], b: &[char]) -> usize {
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    // Iterate with the shorter dimension along the row.
    let (a, b) = if a.len() > b.len() { (b, a) } else { (a, b) };
    let mut prev: Vec<usize> = (0..=a.len()).collect();
    let mut curr: Vec<usize> = vec![0; a.len() + 1];
    for j in 1..=b.len() {
        curr[0] = j;
        for i in 1..=a.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            let del = prev[i] + 1;
            let ins = curr[i - 1] + 1;
            let sub = prev[i - 1] + cost;
            curr[i] = del.min(ins).min(sub);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[a.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn empty_inputs() {
        assert_eq!(levenshtein(&[], &[]), 0);
        assert_eq!(levenshtein(&[], &chars("abc")), 3);
        assert_eq!(levenshtein(&chars("abc"), &[]), 3);
    }

    #[test]
    fn classic_cases() {
        assert_eq!(levenshtein(&chars("kitten"), &chars("sitting")), 3);
        assert_eq!(levenshtein(&chars("flaw"), &chars("lawn")), 2);
        assert_eq!(levenshtein(&chars("abc"), &chars("abc")), 0);
    }

    #[test]
    fn transposition_costs_two() {
        // Levenshtein doesn't model transposition; "ab"→"ba" costs 2.
        assert_eq!(levenshtein(&chars("ab"), &chars("ba")), 2);
    }

    #[test]
    fn case_sensitivity_documents_caller_responsibility() {
        // The function treats inputs verbatim.
        assert_eq!(levenshtein(&chars("Abc"), &chars("abc")), 1);
    }
}
