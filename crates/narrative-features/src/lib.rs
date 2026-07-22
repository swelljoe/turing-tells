//! Narrative-feature annotation: ask a model StoryScope's 30 core questions
//! about a passage, and measure whether its answers can be trusted.
//!
//! Two backends, because the raters we care about are reached two ways: a
//! local llama-server over HTTP (schema-constrained, so malformed answers are
//! impossible) and an arbitrary shell command for frontier CLIs (`claude -p`,
//! `codex exec`), where the schema can only be requested, not enforced.
//!
//! Results are appended to JSONL as they arrive and keyed by
//! (passage, rater, run), so an interrupted sweep resumes without re-spending
//! tokens on work already done.

pub mod agree;
pub mod features;
pub mod taxonomy;

use std::collections::{BTreeSet, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use agree::Val;
use features::{Feature, Kind};

/// One rater's answers to the 30 questions about one passage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub passage: String,
    /// Rater identity: the model, not the machine. Two endpoints running the
    /// same weights are the same rater, so their runs pool for self-consistency.
    pub rater: String,
    pub run: usize,
    pub values: serde_json::Map<String, Value>,
    /// Wall-clock seconds for the call, for the cost side of the question.
    pub seconds: f64,
}

impl Annotation {
    pub fn key(&self) -> (String, String, usize) {
        (self.passage.clone(), self.rater.clone(), self.run)
    }

    /// Typed answer for one feature, or `None` if absent or the wrong shape.
    pub fn value(&self, f: &Feature) -> Option<Val> {
        let raw = self.values.get(f.key)?;
        match f.kind {
            Kind::Scale | Kind::Ordinal => {
                let n = raw.as_i64()?;
                (1..=f.options.len() as i64).contains(&n).then_some(Val::Num(n))
            }
            Kind::Cat => {
                let s = raw.as_str()?;
                f.options.contains(&s).then(|| Val::Cat(s.to_string()))
            }
            Kind::Multi => {
                let items: BTreeSet<String> = raw
                    .as_array()?
                    .iter()
                    .filter_map(|v| v.as_str())
                    .filter(|s| f.options.contains(s))
                    .map(String::from)
                    .collect();
                Some(Val::Set(items))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum Backend {
    /// OpenAI-compatible /v1/chat/completions with JSON-schema decoding.
    Endpoint(String),
    /// Shell command; prompt on stdin, JSON on stdout.
    Command(String),
}

impl Backend {
    pub fn label(&self) -> &str {
        match self {
            Backend::Endpoint(e) => e,
            Backend::Command(c) => c,
        }
    }
}

fn prompt(passage: &str) -> String {
    format!("{}\nPassage:\n\n{passage}", features::rubric())
}

fn chat_payload(passage: &str) -> Value {
    json!({
        "temperature": 1.0,
        "top_p": 0.95,
        "top_k": 64,
        "response_format": {
            "type": "json_schema",
            "json_schema": {"name": "narrative_features", "schema": features::response_schema()}
        },
        "messages": [{"role": "user", "content": prompt(passage)}]
    })
}

fn http_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .proxy(None)
        .timeout_global(Some(std::time::Duration::from_secs(1800)))
        .build()
        .new_agent()
}

/// Pull the first balanced JSON object out of a model's reply. Frontier CLIs
/// wrap answers in prose or code fences however much you ask them not to, and
/// a sweep that dies on a stray "Here you go:" wastes the whole run.
pub fn carve_json(text: &str) -> Result<Value> {
    if let Ok(v) = serde_json::from_str::<Value>(text.trim()) {
        return Ok(v);
    }
    let bytes = text.as_bytes();
    let start = text.find('{').ok_or_else(|| anyhow!("no JSON object in reply: {text:.400}"))?;
    let (mut depth, mut in_str, mut escaped) = (0usize, false, false);
    for i in start..bytes.len() {
        let c = bytes[i] as char;
        if in_str {
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_str = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return serde_json::from_str(&text[start..=i])
                        .with_context(|| format!("unparseable JSON object: {:.400}", &text[start..=i]));
                }
            }
            _ => {}
        }
    }
    Err(anyhow!("unterminated JSON object in reply: {text:.400}"))
}

fn annotate_endpoint(endpoint: &str, passage: &str) -> Result<Value> {
    let url = format!("{}/v1/chat/completions", endpoint.trim_end_matches('/'));
    let mut resp = http_agent()
        .post(&url)
        .send_json(chat_payload(passage))
        .with_context(|| format!("POST {url}"))?;
    let body: Value = resp.body_mut().read_json().context("reading response body")?;
    let content = body["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| anyhow!("no message content: {body}"))?;
    carve_json(content)
}

fn annotate_command(cmd: &str, passage: &str) -> Result<Value> {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawning {cmd}"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("no stdin"))?
        .write_all(prompt(passage).as_bytes())
        .context("writing prompt")?;
    let out = child.wait_with_output().context("waiting for rater")?;
    if !out.status.success() {
        return Err(anyhow!(
            "{cmd} exited {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    carve_json(&String::from_utf8_lossy(&out.stdout))
}

/// One annotation call. `Value` rather than `Annotation` because the caller
/// owns the identity fields.
pub fn annotate(backend: &Backend, passage: &str) -> Result<Value> {
    match backend {
        Backend::Endpoint(e) => annotate_endpoint(e, passage),
        Backend::Command(c) => annotate_command(c, passage),
    }
}

/// Missing or wrongly-shaped answers, for judging a rater's compliance
/// separately from its agreement. A model that cannot produce the schema is a
/// different failure from one that produces it and disagrees.
pub fn malformed(values: &serde_json::Map<String, Value>) -> Vec<&'static str> {
    let probe = Annotation {
        passage: String::new(),
        rater: String::new(),
        run: 0,
        values: values.clone(),
        seconds: 0.0,
    };
    features::all().iter().filter(|f| probe.value(f).is_none()).map(|f| f.key).collect()
}

pub fn load(path: &Path) -> Result<Vec<Annotation>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for (i, line) in BufReader::new(File::open(path)?).lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        out.push(
            serde_json::from_str(&line)
                .with_context(|| format!("{}:{}", path.display(), i + 1))?,
        );
    }
    Ok(out)
}

/// Append-only sink shared by every worker.
pub struct Sink(Mutex<std::io::BufWriter<File>>);

impl Sink {
    pub fn open(path: &Path) -> Result<Self> {
        let f = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Sink(Mutex::new(std::io::BufWriter::new(f))))
    }

    pub fn write(&self, a: &Annotation) -> Result<()> {
        let mut w = self.0.lock().expect("sink poisoned");
        writeln!(w, "{}", serde_json::to_string(a)?)?;
        w.flush()?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Job {
    pub passage: String,
    pub text: String,
    pub run: usize,
}

/// Run every job across the given backends concurrently, one worker per
/// backend. Backends are assumed to be the same rater (same weights, different
/// machines); work is pulled rather than partitioned, so a slower box simply
/// takes fewer jobs instead of holding up the sweep.
///
/// `on_result` sees every outcome, success or failure, for progress reporting
/// and for the caller's own abort logic.
pub fn sweep(
    jobs: Vec<Job>,
    backends: &[Backend],
    rater: &str,
    sink: &Sink,
    on_result: impl Fn(&Job, &Result<Annotation>) + Sync,
) -> Result<()> {
    anyhow::ensure!(!backends.is_empty(), "no backends");
    let queue = Arc::new(Mutex::new(jobs));
    let on_result = &on_result;
    std::thread::scope(|scope| {
        for backend in backends {
            let queue = Arc::clone(&queue);
            scope.spawn(move || {
                loop {
                    let job = { queue.lock().expect("queue poisoned").pop() };
                    let Some(job) = job else { break };
                    let start = std::time::Instant::now();
                    let result = annotate(backend, &job.text).and_then(|v| {
                        Ok(Annotation {
                            passage: job.passage.clone(),
                            rater: rater.to_string(),
                            run: job.run,
                            values: v
                                .as_object()
                                .ok_or_else(|| anyhow!("reply was not a JSON object"))?
                                .clone(),
                            seconds: start.elapsed().as_secs_f64(),
                        })
                    });
                    if let Ok(a) = &result
                        && let Err(e) = sink.write(a)
                    {
                        eprintln!("warning: could not record {}: {e}", a.passage);
                    }
                    on_result(&job, &result);
                }
            });
        }
    });
    Ok(())
}

/// Jobs still outstanding given what is already on disk.
pub fn remaining(
    passages: &[(String, String)],
    runs: usize,
    rater: &str,
    done: &[Annotation],
) -> Vec<Job> {
    let have: HashSet<(String, String, usize)> = done.iter().map(Annotation::key).collect();
    let mut jobs = Vec::new();
    for (name, text) in passages {
        for run in 1..=runs {
            if !have.contains(&(name.clone(), rater.to_string(), run)) {
                jobs.push(Job { passage: name.clone(), text: text.clone(), run });
            }
        }
    }
    jobs
}

/// Collapse each rater's repeated runs on a passage into one consensus
/// annotation, recorded as run 0. Sampling noise is the cheapest kind of rater
/// error to fix -- pay for N calls instead of one -- so a rater that is
/// unreliable single-shot may still be usable pooled. Whether it is, is an
/// empirical question this makes askable.
///
/// Numerics take the median, categoricals the mode, multi-selects the options
/// carried by at least half the runs. Ties resolve to the lexicographically
/// smallest answer, which is arbitrary but deterministic.
pub fn aggregate(annotations: &[Annotation]) -> Vec<Annotation> {
    let mut groups: std::collections::BTreeMap<(String, String), Vec<&Annotation>> =
        std::collections::BTreeMap::new();
    for a in annotations {
        groups.entry((a.passage.clone(), a.rater.clone())).or_default().push(a);
    }

    groups
        .into_iter()
        .map(|((passage, rater), runs)| {
            let mut values = serde_json::Map::new();
            for f in features::all() {
                let answers: Vec<Val> = runs.iter().filter_map(|a| a.value(f)).collect();
                if answers.is_empty() {
                    continue;
                }
                let consensus = match f.kind {
                    Kind::Scale | Kind::Ordinal => {
                        let mut ns: Vec<i64> = answers.iter().filter_map(Val::as_num).collect();
                        ns.sort_unstable();
                        // Lower median on even counts: always a value someone
                        // actually gave, and never off the scale.
                        json!(ns[(ns.len() - 1) / 2])
                    }
                    Kind::Cat => {
                        let mut counts: std::collections::BTreeMap<&str, usize> =
                            std::collections::BTreeMap::new();
                        for a in &answers {
                            if let Val::Cat(s) = a {
                                *counts.entry(s.as_str()).or_default() += 1;
                            }
                        }
                        match counts.iter().max_by_key(|(s, n)| (**n, std::cmp::Reverse(**s))) {
                            Some((s, _)) => json!(s),
                            None => continue,
                        }
                    }
                    Kind::Multi => {
                        let mut counts: std::collections::BTreeMap<&str, usize> =
                            std::collections::BTreeMap::new();
                        for a in &answers {
                            if let Val::Set(items) = a {
                                for i in items {
                                    *counts.entry(i.as_str()).or_default() += 1;
                                }
                            }
                        }
                        let kept: Vec<&str> = counts
                            .into_iter()
                            .filter(|(_, n)| n * 2 >= answers.len())
                            .map(|(s, _)| s)
                            .collect();
                        json!(kept)
                    }
                };
                values.insert(f.key.to_string(), consensus);
            }
            let seconds = runs.iter().map(|a| a.seconds).sum();
            Annotation { passage, rater, run: 0, values, seconds }
        })
        .collect()
}

/// Agreement per feature across the given annotations. `group` names each
/// annotation's rater identity for the comparison at hand: the rater name when
/// comparing models, the run number when measuring one model's self-consistency.
pub fn report(annotations: &[Annotation]) -> Vec<agree::Report> {
    let mut passages: Vec<&str> = annotations.iter().map(|a| a.passage.as_str()).collect();
    passages.sort_unstable();
    passages.dedup();

    features::all()
        .iter()
        .map(|f| {
            let units: Vec<Vec<Val>> = passages
                .iter()
                .map(|p| {
                    annotations
                        .iter()
                        .filter(|a| a.passage == *p)
                        .filter_map(|a| a.value(f))
                        .collect()
                })
                .collect();
            let mut seen: Vec<String> =
                units.iter().flatten().map(|v| format!("{v:?}")).collect();
            seen.sort_unstable();
            seen.dedup();
            agree::Report {
                feature: f,
                alpha: agree::alpha(f.kind, &units),
                percent: agree::percent(f.kind, &units),
                distinct: seen.len(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ann(passage: &str, rater: &str, run: usize, values: Value) -> Annotation {
        Annotation {
            passage: passage.into(),
            rater: rater.into(),
            run,
            values: values.as_object().unwrap().clone(),
            seconds: 1.0,
        }
    }

    #[test]
    fn json_is_carved_out_of_chatty_replies() {
        let v = carve_json("Sure! Here you go:\n```json\n{\"a\": 1}\n```\nHope that helps.").unwrap();
        assert_eq!(v["a"], 1);
    }

    #[test]
    fn carving_survives_braces_inside_strings() {
        let v = carve_json(r#"prefix {"a": "} not the end {", "b": 2} suffix"#).unwrap();
        assert_eq!(v["b"], 2);
        assert_eq!(v["a"], "} not the end {");
    }

    #[test]
    fn carving_survives_escaped_quotes() {
        let v = carve_json(r#"{"a": "she said \"hi\"", "b": 3}"#).unwrap();
        assert_eq!(v["b"], 3);
    }

    #[test]
    fn unterminated_json_is_an_error_not_a_partial_answer() {
        assert!(carve_json("{\"a\": 1").is_err());
        assert!(carve_json("no json here").is_err());
    }

    #[test]
    fn answers_are_typed_against_the_feature_table() {
        let f = features::find("thematic_explicitness").unwrap();
        let good = ann("p", "r", 1, json!({"thematic_explicitness": 4}));
        assert_eq!(good.value(f), Some(Val::Num(4)));
        // Out of range and wrong shape are both rejected, not clamped.
        assert_eq!(ann("p", "r", 1, json!({"thematic_explicitness": 9})).value(f), None);
        assert_eq!(ann("p", "r", 1, json!({"thematic_explicitness": "4"})).value(f), None);
    }

    #[test]
    fn categorical_answers_outside_the_option_set_are_rejected() {
        let f = features::find("moral_polarity").unwrap();
        assert!(ann("p", "r", 1, json!({"moral_polarity": "ambivalent_mixed"})).value(f).is_some());
        assert_eq!(ann("p", "r", 1, json!({"moral_polarity": "cheerful"})).value(f), None);
    }

    #[test]
    fn multi_select_drops_invented_options_and_keeps_the_rest() {
        let f = features::find("sensory_modalities").unwrap();
        let a = ann("p", "r", 1, json!({"sensory_modalities": ["visual", "telepathic"]}));
        assert_eq!(a.value(f), Some(Val::Set(["visual".to_string()].into_iter().collect())));
    }

    #[test]
    fn malformed_lists_every_unanswered_feature() {
        let values = json!({"thematic_explicitness": 3}).as_object().unwrap().clone();
        let missing = malformed(&values);
        assert_eq!(missing.len(), 29);
        assert!(!missing.contains(&"thematic_explicitness"));
    }

    #[test]
    fn remaining_skips_work_already_recorded() {
        let passages = vec![("a".to_string(), "text".to_string())];
        let done = vec![ann("a", "gemma", 1, json!({}))];
        let jobs = remaining(&passages, 2, "gemma", &done);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].run, 2);
        // A different rater has done none of it.
        assert_eq!(remaining(&passages, 2, "opus", &done).len(), 2);
    }

    #[test]
    fn aggregation_takes_the_median_of_a_raters_runs() {
        let anns = vec![
            ann("p", "r", 1, json!({"sensory_density": 2})),
            ann("p", "r", 2, json!({"sensory_density": 5})),
            ann("p", "r", 3, json!({"sensory_density": 4})),
        ];
        let agg = aggregate(&anns);
        assert_eq!(agg.len(), 1);
        assert_eq!(agg[0].run, 0);
        assert_eq!(agg[0].values["sensory_density"], 4);
    }

    #[test]
    fn aggregation_keeps_raters_separate() {
        let anns = vec![
            ann("p", "a", 1, json!({"sensory_density": 1})),
            ann("p", "b", 1, json!({"sensory_density": 5})),
        ];
        let agg = aggregate(&anns);
        assert_eq!(agg.len(), 2);
        assert_eq!(agg[0].values["sensory_density"], 1);
        assert_eq!(agg[1].values["sensory_density"], 5);
    }

    #[test]
    fn aggregation_takes_the_mode_of_categoricals() {
        let v = |s| json!({"moral_polarity": s});
        let anns = vec![
            ann("p", "r", 1, v("clearly_positive")),
            ann("p", "r", 2, v("ambivalent_mixed")),
            ann("p", "r", 3, v("ambivalent_mixed")),
        ];
        assert_eq!(aggregate(&anns)[0].values["moral_polarity"], "ambivalent_mixed");
    }

    #[test]
    fn aggregation_keeps_multi_select_options_carried_by_half_the_runs() {
        let v = |s: Value| json!({"sensory_modalities": s});
        let anns = vec![
            ann("p", "r", 1, v(json!(["visual", "tactile"]))),
            ann("p", "r", 2, v(json!(["visual", "auditory"]))),
            ann("p", "r", 3, v(json!(["visual"]))),
        ];
        // visual in 3/3 survives; tactile and auditory at 1/3 do not.
        assert_eq!(aggregate(&anns)[0].values["sensory_modalities"], json!(["visual"]));
    }

    #[test]
    fn aggregation_ignores_runs_that_failed_to_answer_a_feature() {
        let anns = vec![
            ann("p", "r", 1, json!({"sensory_density": 3})),
            ann("p", "r", 2, json!({"sensory_density": 99})),
        ];
        // The out-of-range answer is dropped, leaving the one valid reading.
        assert_eq!(aggregate(&anns)[0].values["sensory_density"], 3);
    }

    #[test]
    fn report_measures_agreement_between_raters_on_each_passage() {
        let vals = |n| json!({"thematic_explicitness": n});
        let anns = vec![
            ann("p1", "a", 1, vals(5)),
            ann("p1", "b", 1, vals(5)),
            ann("p2", "a", 1, vals(1)),
            ann("p2", "b", 1, vals(1)),
        ];
        let r = report(&anns);
        let te = r.iter().find(|r| r.feature.key == "thematic_explicitness").unwrap();
        assert_eq!(te.alpha, Some(1.0));
        assert_eq!(te.percent.exact, 1.0);
        assert_eq!(te.distinct, 2);
        // Features nobody answered carry no agreement signal.
        let other = r.iter().find(|r| r.feature.key == "moral_polarity").unwrap();
        assert_eq!(other.alpha, None);
        assert_eq!(other.percent.pairs, 0);
    }
}
