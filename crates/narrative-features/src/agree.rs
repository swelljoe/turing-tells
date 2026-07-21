//! Inter-rater agreement.
//!
//! The question this crate exists to answer — "can a model be trusted to read
//! these features off a passage?" — has no ground truth, so it has to be
//! answered by agreement: between independent runs of one model
//! (self-consistency) and between different models (concordance).
//!
//! Percent agreement alone would flatter features whose answer is nearly always
//! the same, so the headline statistic is Krippendorff's alpha, which corrects
//! for the agreement you would get by chance given the observed distribution.
//! Alpha handles the three answer shapes here through its difference function:
//! squared distance for scales and ordinals, 0/1 for categoricals, Jaccard for
//! multi-selects.

use std::collections::BTreeSet;

use crate::features::{Feature, Kind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Val {
    Num(i64),
    Cat(String),
    Set(BTreeSet<String>),
}

impl Val {
    pub fn as_num(&self) -> Option<i64> {
        match self {
            Val::Num(n) => Some(*n),
            _ => None,
        }
    }
}

/// Difference between two answers, in [0, inf). Mismatched shapes are maximal
/// disagreement rather than a panic: a rater that answers the wrong type is
/// disagreeing in the most complete way available.
pub fn delta(kind: Kind, a: &Val, b: &Val) -> f64 {
    match (a, b) {
        (Val::Num(x), Val::Num(y)) => {
            let d = (x - y) as f64;
            d * d
        }
        (Val::Cat(x), Val::Cat(y)) => f64::from(x != y),
        (Val::Set(x), Val::Set(y)) => {
            let union = x.union(y).count();
            if union == 0 {
                0.0 // both empty: identical answers
            } else {
                1.0 - x.intersection(y).count() as f64 / union as f64
            }
        }
        _ => match kind {
            Kind::Scale | Kind::Ordinal => 16.0,
            _ => 1.0,
        },
    }
}

/// Krippendorff's alpha over units of ratings. Each inner slice is one
/// passage's answers from every rater; units with fewer than two answers carry
/// no agreement information and are skipped.
///
/// Returns `None` when alpha is undefined: fewer than two comparable values, or
/// zero expected disagreement, which happens when every rater gave the same
/// answer to every passage. That case is unanimity, not reliability — a feature
/// that is constant across the corpus tells us nothing — so it is reported as
/// undefined rather than as 1.0.
pub fn alpha(kind: Kind, units: &[Vec<Val>]) -> Option<f64> {
    let usable: Vec<&Vec<Val>> = units.iter().filter(|u| u.len() >= 2).collect();
    let all: Vec<&Val> = usable.iter().flat_map(|u| u.iter()).collect();
    let n = all.len();
    if n < 2 {
        return None;
    }

    let mut observed = 0.0;
    for u in &usable {
        let m = u.len() as f64;
        let mut sum = 0.0;
        for a in u.iter() {
            for b in u.iter() {
                sum += delta(kind, a, b);
            }
        }
        observed += sum / (m - 1.0);
    }
    observed /= n as f64;

    let mut expected = 0.0;
    for a in &all {
        for b in &all {
            expected += delta(kind, a, b);
        }
    }
    // The ordered-pair sums above include the n self-pairs, whose delta is 0,
    // so no correction is needed beyond the n(n-1) denominator.
    expected /= (n * (n - 1)) as f64;

    if expected == 0.0 {
        return None;
    }
    Some(1.0 - observed / expected)
}

/// Fraction of rater pairs answering identically, and — for numeric features —
/// the fraction landing within one scale point. Alpha is the statistic to
/// trust, but a raw percentage is what makes it legible.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Percent {
    pub exact: f64,
    pub within_one: Option<f64>,
    pub pairs: usize,
}

pub fn percent(kind: Kind, units: &[Vec<Val>]) -> Percent {
    let (mut pairs, mut exact, mut near) = (0usize, 0usize, 0usize);
    for u in units.iter().filter(|u| u.len() >= 2) {
        for i in 0..u.len() {
            for j in (i + 1)..u.len() {
                pairs += 1;
                if u[i] == u[j] {
                    exact += 1;
                }
                if let (Some(x), Some(y)) = (u[i].as_num(), u[j].as_num())
                    && (x - y).abs() <= 1
                {
                    near += 1;
                }
            }
        }
    }
    if pairs == 0 {
        return Percent::default();
    }
    let frac = |k: usize| k as f64 / pairs as f64;
    Percent {
        exact: frac(exact),
        within_one: matches!(kind, Kind::Scale | Kind::Ordinal).then(|| frac(near)),
        pairs,
    }
}

#[derive(Debug, Clone)]
pub struct Report {
    pub feature: &'static Feature,
    pub alpha: Option<f64>,
    pub percent: Percent,
    /// Distinct answers seen. A feature with one answer everywhere is not
    /// measuring anything on this corpus, whatever its agreement looks like.
    pub distinct: usize,
}

/// Alpha's conventional reading: .800 for conclusions, .667 for tentative
/// ones. Below that, the annotation is not reliable enough to build on.
pub fn verdict(a: Option<f64>) -> &'static str {
    match a {
        None => "undefined",
        Some(a) if a >= 0.800 => "reliable",
        Some(a) if a >= 0.667 => "tentative",
        Some(a) if a > 0.0 => "unreliable",
        Some(_) => "no better than chance",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat(s: &str) -> Val {
        Val::Cat(s.to_string())
    }
    fn set(items: &[&str]) -> Val {
        Val::Set(items.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn perfect_agreement_on_a_varying_feature_is_alpha_one() {
        let units = vec![vec![cat("a"), cat("a")], vec![cat("b"), cat("b")]];
        assert_eq!(alpha(Kind::Cat, &units), Some(1.0));
    }

    #[test]
    fn total_disagreement_is_hand_checkable() {
        // Two units, two raters, systematically opposed:
        // Do = 1, De = 8/12, alpha = 1 - 1/(2/3) = -0.5.
        let units = vec![vec![cat("a"), cat("b")], vec![cat("a"), cat("b")]];
        let a = alpha(Kind::Cat, &units).unwrap();
        assert!((a - -0.5).abs() < 1e-12, "got {a}");
    }

    #[test]
    fn unanimity_across_the_corpus_is_undefined_not_perfect() {
        let units = vec![vec![cat("a"), cat("a")], vec![cat("a"), cat("a")]];
        assert_eq!(alpha(Kind::Cat, &units), None);
    }

    #[test]
    fn a_single_rater_per_passage_yields_no_agreement_signal() {
        let units = vec![vec![cat("a")], vec![cat("b")]];
        assert_eq!(alpha(Kind::Cat, &units), None);
        assert_eq!(percent(Kind::Cat, &units).pairs, 0);
    }

    #[test]
    fn scales_penalise_distant_answers_more_than_adjacent_ones() {
        let near = vec![vec![Val::Num(3), Val::Num(4)], vec![Val::Num(1), Val::Num(2)]];
        let far = vec![vec![Val::Num(1), Val::Num(5)], vec![Val::Num(5), Val::Num(1)]];
        let (n, f) = (alpha(Kind::Scale, &near).unwrap(), alpha(Kind::Scale, &far).unwrap());
        assert!(n > f, "adjacent {n} should beat opposed {f}");
    }

    #[test]
    fn multi_select_agreement_is_jaccard() {
        assert_eq!(delta(Kind::Multi, &set(&["a", "b"]), &set(&["a", "b"])), 0.0);
        assert_eq!(delta(Kind::Multi, &set(&["a", "b"]), &set(&["a"])), 0.5);
        assert_eq!(delta(Kind::Multi, &set(&["a"]), &set(&["b"])), 1.0);
        assert_eq!(delta(Kind::Multi, &set(&[]), &set(&[])), 0.0);
    }

    #[test]
    fn mismatched_answer_shapes_count_as_maximal_disagreement() {
        assert_eq!(delta(Kind::Cat, &cat("a"), &Val::Num(1)), 1.0);
        assert_eq!(delta(Kind::Scale, &Val::Num(1), &cat("a")), 16.0);
    }

    #[test]
    fn percent_reports_exact_and_adjacent_rates() {
        let units = vec![vec![Val::Num(3), Val::Num(3), Val::Num(4)]];
        let p = percent(Kind::Scale, &units);
        assert_eq!(p.pairs, 3);
        assert!((p.exact - 1.0 / 3.0).abs() < 1e-12);
        assert_eq!(p.within_one, Some(1.0));
    }

    #[test]
    fn categorical_features_report_no_adjacency() {
        assert_eq!(percent(Kind::Cat, &[vec![cat("a"), cat("b")]]).within_one, None);
    }

    #[test]
    fn verdict_tracks_the_conventional_thresholds() {
        assert_eq!(verdict(Some(0.85)), "reliable");
        assert_eq!(verdict(Some(0.70)), "tentative");
        assert_eq!(verdict(Some(0.30)), "unreliable");
        assert_eq!(verdict(Some(-0.1)), "no better than chance");
        assert_eq!(verdict(None), "undefined");
    }
}
