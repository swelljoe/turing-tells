//! Load StoryScope's released feature taxonomy so we annotate with their exact
//! feature IDs, question wording, and value sets. Extracting against their own
//! taxonomy is what makes our values comparable to their published parquet
//! value-for-value — the point of the reproduction.
//!
//! Their taxonomy is not vendored (it is their released artifact); the path is
//! supplied at runtime.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

use crate::features::{Feature, Kind};

#[derive(Debug, Deserialize)]
struct TaxonomyFile {
    feature_taxonomy: BTreeMap<String, Dimension>,
}

#[derive(Debug, Deserialize)]
struct Dimension {
    aspects: BTreeMap<String, Aspect>,
}

#[derive(Debug, Deserialize)]
struct Aspect {
    features: Vec<RawFeature>,
}

#[derive(Debug, Deserialize)]
struct RawFeature {
    id: String,
    #[allow(dead_code)]
    name: String,
    question: String,
    #[serde(rename = "type")]
    kind: String,
    /// Scale values arrive as JSON integers, everything else as strings.
    values: Vec<serde_json::Value>,
}

/// One taxonomy entry, decoupled from the `&'static` `Feature` so callers can
/// hold it before deciding to leak it. `dim` is the NarraBench dimension the
/// feature belongs to.
pub struct Loaded {
    pub id: String,
    pub name: String,
    pub question: String,
    pub dim: String,
    pub kind: Kind,
    pub options: Vec<String>,
}

fn kind_of(raw: &str) -> Result<Kind> {
    Ok(match raw {
        "scale" => Kind::Scale,
        "ordinal" => Kind::Ordinal,
        // Binary is a two-option categorical; the parquet stores yes/no.
        "categorical" | "binary" => Kind::Cat,
        "multi_select" => Kind::Multi,
        other => return Err(anyhow!("unknown feature type {other:?}")),
    })
}

fn options_of(kind: Kind, values: &[serde_json::Value]) -> Vec<String> {
    match kind {
        // Scale options are the labels 1..=N; the schema and the published
        // parquet both use those integers as strings.
        Kind::Scale => values
            .iter()
            .map(|v| v.as_i64().map(|n| n.to_string()).unwrap_or_else(|| v.to_string()))
            .collect(),
        _ => values
            .iter()
            .map(|v| v.as_str().map(String::from).unwrap_or_else(|| v.to_string()))
            .collect(),
    }
}

/// Every feature in the taxonomy, in dimension/aspect/id order.
pub fn load(path: &Path) -> Result<Vec<Loaded>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading taxonomy {}", path.display()))?;
    let file: TaxonomyFile = serde_json::from_str(&text)
        .with_context(|| format!("parsing taxonomy {}", path.display()))?;
    let mut out = Vec::new();
    for (dim, d) in &file.feature_taxonomy {
        for aspect in d.aspects.values() {
            for f in &aspect.features {
                let kind = kind_of(&f.kind)
                    .with_context(|| format!("feature {}", f.id))?;
                out.push(Loaded {
                    id: f.id.clone(),
                    name: f.name.clone(),
                    question: f.question.clone(),
                    dim: dim.clone(),
                    kind,
                    options: options_of(kind, &f.values),
                });
            }
        }
    }
    Ok(out)
}

/// Leak a loaded feature into a `&'static Feature`. Leaking is deliberate: the
/// active feature set lives for the whole process, and making it `'static`
/// keeps every downstream signature (schemas, reports, `&'static Feature` in
/// findings) unchanged from the built-in path.
fn leak(f: Loaded) -> Feature {
    let opts: Vec<&'static str> =
        f.options.into_iter().map(|s| &*Box::leak(s.into_boxed_str())).collect();
    Feature {
        key: Box::leak(f.id.into_boxed_str()),
        name: Box::leak(f.name.into_boxed_str()),
        question: Box::leak(f.question.into_boxed_str()),
        dim: Box::leak(f.dim.into_boxed_str()),
        kind: f.kind,
        options: Box::leak(opts.into_boxed_slice()),
    }
}

/// Load the taxonomy and optionally narrow it to `subset` (a list of feature
/// IDs, order preserved). An unknown ID in `subset` is an error, not a silent
/// skip: a typo would quietly shrink the study. Pure — installs nothing.
pub fn build(path: &Path, subset: Option<&[String]>) -> Result<Vec<Feature>> {
    let loaded = load(path)?;
    let by_id: BTreeMap<&str, usize> =
        loaded.iter().enumerate().map(|(i, f)| (f.id.as_str(), i)).collect();

    let chosen: Vec<Feature> = match subset {
        None => loaded.into_iter().map(leak).collect(),
        Some(ids) => {
            let picks: Result<Vec<usize>> = ids
                .iter()
                .map(|id| {
                    by_id
                        .get(id.as_str())
                        .copied()
                        .ok_or_else(|| anyhow!("feature id {id:?} is not in the taxonomy"))
                })
                .collect();
            let picks = picks?;
            // Move out only the chosen indices, preserving subset order.
            let mut slots: Vec<Option<Loaded>> = loaded.into_iter().map(Some).collect();
            picks
                .into_iter()
                .map(|i| leak(slots[i].take().expect("duplicate id in subset")))
                .collect()
        }
    };
    anyhow::ensure!(!chosen.is_empty(), "taxonomy produced no features");
    Ok(chosen)
}

/// Build the feature set and install it as active for this run.
pub fn activate(path: &Path, subset: Option<&[String]>) -> Result<&'static [Feature]> {
    let leaked: &'static [Feature] = Box::leak(build(path, subset)?.into_boxed_slice());
    crate::features::set_active(leaked)?;
    Ok(leaked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const SAMPLE: &str = r#"{
      "feature_taxonomy": {
        "revelation": {"aspects": {"suspense": {"features": [
          {"id":"REV_SUS_001","name":"Withholding","question":"How much?","type":"ordinal","values":["none","low","high"]},
          {"id":"REV_SUS_007","name":"Pace","question":"How fast?","type":"scale","values":[1,2,3,4,5]}
        ]}}},
        "style": {"aspects": {"figure": {"features": [
          {"id":"STY_FIG_004","name":"Conceit","question":"Present?","type":"binary","values":["no","yes"]},
          {"id":"STY_FIG_002","name":"Devices","question":"Which?","type":"multi_select","values":["simile","metaphor"]}
        ]}}}
      }
    }"#;

    fn write_tmp(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir();
        let p = dir.join(format!("tax-test-{}-{tag}.json", std::process::id()));
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(SAMPLE.as_bytes()).unwrap();
        p
    }

    #[test]
    fn loads_every_feature_with_mapped_kinds() {
        const TAG: &str = "load";
        let p = write_tmp(TAG);
        let fs = load(&p).unwrap();
        assert_eq!(fs.len(), 4);
        let by = |id| fs.iter().find(|f: &&Loaded| f.id == id).unwrap();
        assert_eq!(by("REV_SUS_001").kind, Kind::Ordinal);
        assert_eq!(by("REV_SUS_007").kind, Kind::Scale);
        assert_eq!(by("REV_SUS_007").options, vec!["1", "2", "3", "4", "5"]);
        assert_eq!(by("STY_FIG_004").kind, Kind::Cat); // binary -> cat
        assert_eq!(by("STY_FIG_004").options, vec!["no", "yes"]);
        assert_eq!(by("STY_FIG_002").kind, Kind::Multi);
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn a_subset_preserves_request_order() {
        const TAG: &str = "order";
        let p = write_tmp(TAG);
        let ids = vec!["STY_FIG_004".to_string(), "REV_SUS_001".to_string()];
        let built = build(&p, Some(&ids)).unwrap();
        assert_eq!(built.iter().map(|f| f.key).collect::<Vec<_>>(), ["STY_FIG_004", "REV_SUS_001"]);
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn an_unknown_subset_id_is_an_error() {
        const TAG: &str = "typo";
        let p = write_tmp(TAG);
        let ids = vec!["NOPE_001".to_string()];
        let err = build(&p, Some(&ids)).unwrap_err().to_string();
        assert!(err.contains("NOPE_001"), "got: {err}");
        std::fs::remove_file(p).ok();
    }
}
