//! The 30 core narrative features StoryScope found separating human from AI
//! fiction (their Tables 13-15), transcribed with the paper's own question
//! wording and response options.
//!
//! Feature *names* here are ours; the questions are theirs, because the
//! question is the measuring instrument. Changing the wording would make our
//! numbers incomparable to theirs, which is the whole point of the exercise.

use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// 1-5 Likert. `labels` name the poles when the paper gave them.
    Scale,
    /// Ordered categories; the paper means over integer codes, so order matters.
    Ordinal,
    /// Pick exactly one.
    Cat,
    /// Pick any number, including none.
    Multi,
}

#[derive(Debug, Clone, Copy)]
pub struct Feature {
    pub key: &'static str,
    pub name: &'static str,
    pub question: &'static str,
    /// NarraBench dimension prefix, kept so per-dimension rollups stay possible.
    pub dim: &'static str,
    pub kind: Kind,
    pub options: &'static [&'static str],
}

impl Feature {
    /// Scales and ordinals are integers; the schema bounds them by option count.
    pub fn numeric(&self) -> bool {
        matches!(self.kind, Kind::Scale | Kind::Ordinal)
    }
}

/// Table 15's 30 features, in its grouping order. Six of these appear in both
/// Table 13 and Table 14 with different elevated options (e.g. Reference
/// Explicitness is AI-elevated at "implicit echoes" and human-elevated at
/// "balanced mix"); they are one question each, asked once.
pub const FEATURES: &[Feature] = &[
    // AI-elevated: thematic over-determination
    Feature {
        key: "thematic_explicitness",
        name: "Thematic Explicitness and Moralizing",
        question: "How explicitly does the story articulate its themes or morals?",
        dim: "SIT",
        kind: Kind::Scale,
        options: &["1", "2", "3", "4", "5"],
    },
    Feature {
        key: "moral_weighting",
        name: "Moral / Philosophical Weighting",
        question: "How heavily does the story foreground moral or philosophical questions?",
        dim: "SIT",
        kind: Kind::Scale,
        options: &["1", "2", "3", "4", "5"],
    },
    Feature {
        key: "thematic_unity",
        name: "Thematic Unity",
        question: "To what extent do subplots and flourishes serve a central thematic concern?",
        dim: "PLT",
        kind: Kind::Scale,
        options: &["1", "2", "3", "4", "5"],
    },
    Feature {
        key: "narratorial_commentary",
        name: "Narratorial Thematic Commentary",
        question: "Does the narrator explicitly comment on themes beyond the narrative?",
        dim: "SIT",
        kind: Kind::Cat,
        options: &["no", "yes"],
    },
    Feature {
        key: "dialogue_function",
        name: "Dialogue Function",
        question: "What main functions does dialogue serve?",
        dim: "PER",
        kind: Kind::Multi,
        options: &[
            "advance_plot",
            "reveal_character",
            "worldbuilding",
            "philosophical",
            "comic",
        ],
    },
    Feature {
        key: "reference_explicitness",
        name: "Reference Explicitness",
        question: "Are intertextual gestures primarily explicit or diffuse?",
        dim: "SIT",
        kind: Kind::Cat,
        options: &["none", "explicit_named", "implicit_echoes", "balanced_mix"],
    },
    // AI-elevated: sensory & embodied performativity
    Feature {
        key: "emotional_expression",
        name: "Dominant Emotional Expression",
        question: "How are characters' emotions most commonly conveyed?",
        dim: "AGENT",
        kind: Kind::Cat,
        options: &["explicit_labels", "embodied_metaphors", "behavioral_cues", "ambiguous"],
    },
    Feature {
        key: "setting_psychological_mirror",
        name: "Setting as Psychological Mirror",
        question: "To what degree does physical environment mirror characters' inner states?",
        dim: "SET",
        kind: Kind::Scale,
        options: &["1", "2", "3", "4", "5"],
    },
    Feature {
        key: "environmental_emphasis",
        name: "Environmental and Ecological Emphasis",
        question: "How prominent is the natural environment or ecology in the narrative?",
        dim: "SET",
        kind: Kind::Scale,
        options: &["1", "2", "3", "4", "5"],
    },
    Feature {
        key: "sensory_modalities",
        name: "Dominant Sensory Modalities",
        question: "Which sensory modalities does the story most frequently engage?",
        dim: "SET",
        kind: Kind::Multi,
        options: &["visual", "auditory", "olfactory", "tactile", "gustatory", "kinesthetic"],
    },
    Feature {
        key: "sensory_density",
        name: "Sensory Density",
        question: "How dense is sensory description? (1 = minimal, 5 = lush)",
        dim: "SET",
        kind: Kind::Scale,
        options: &["1", "2", "3", "4", "5"],
    },
    Feature {
        key: "depth_interior_access",
        name: "Depth of Interior Access",
        question: "How deep into characters' inner life does narration go?",
        dim: "PER",
        kind: Kind::Scale,
        options: &["1", "2", "3", "4", "5"],
    },
    // AI-elevated: structural streamlining
    Feature {
        key: "causal_chain_continuity",
        name: "Continuity of Main Causal Chain",
        question: "How continuous is the single causal chain from inciting incident to ending?",
        dim: "EVT",
        kind: Kind::Scale,
        options: &["1", "2", "3", "4", "5"],
    },
    Feature {
        key: "spatial_granularity",
        name: "Spatial Granularity Level",
        question: "How fine-grained is the story's depiction of physical space?",
        dim: "SET",
        kind: Kind::Ordinal,
        options: &["very_low", "low", "moderate", "high", "very_high"],
    },
    Feature {
        key: "agency_in_resolution",
        name: "Agency in Resolution",
        question: "Is resolution driven by protagonist's choices or external events?",
        dim: "PLT",
        kind: Kind::Cat,
        options: &["protagonist_choice", "mixed", "external_fate"],
    },
    Feature {
        key: "character_introduction",
        name: "Character Introduction",
        question: "Which narrative device primarily introduces the central character?",
        dim: "AGENT",
        kind: Kind::Cat,
        options: &["external_desc", "in_action", "in_dialogue", "inner_thought", "others_reports"],
    },
    Feature {
        key: "subplot_integration",
        name: "Subplot Integration",
        question: "How directly do subplots echo the central theme?",
        dim: "PLT",
        kind: Kind::Cat,
        options: &["no_subplots", "thematically_parallel", "contrasting", "independent"],
    },
    Feature {
        key: "resolution_mode",
        name: "Mode of Resolution",
        question: "Is the main event resolved through internal acceptance or external action?",
        dim: "EVT",
        kind: Kind::Cat,
        options: &["resolved_externally", "resolved_internally", "unresolved"],
    },
    Feature {
        key: "opening_spatial_grounding",
        name: "Opening Spatial Grounding",
        question: "How clearly does the opening ground the reader in a specific physical setting?",
        dim: "SET",
        kind: Kind::Ordinal,
        options: &["none_vague", "minimal", "clear_local", "clear_local_and_global"],
    },
    Feature {
        key: "pre_threat_investment",
        name: "Pre-Threat Character Investment",
        question: "How much does the story build investment before jeopardy?",
        dim: "REV",
        kind: Kind::Scale,
        options: &["1", "2", "3", "4", "5"],
    },
    // Human-elevated: intertextual richness
    Feature {
        key: "intertextual_strategy",
        name: "Intertextual Strategy Types",
        question: "What kinds of intertextual engagement does the story employ?",
        dim: "SIT",
        kind: Kind::Multi,
        options: &["explicit_named", "retelling", "pastiche", "myth_religion", "self_referential"],
    },
    // Human-elevated: reader engagement
    Feature {
        key: "direct_reader_address",
        name: "Frequency of Direct Reader Address",
        question: "How often does the text directly address the reader?",
        dim: "PER",
        kind: Kind::Ordinal,
        options: &["never", "occasional_asides", "frequent", "structural"],
    },
    Feature {
        key: "fourth_wall_permeability",
        name: "Fourth-Wall Permeability",
        question: "To what extent does the story break the boundary between story-world and reader? (1 = no breaking, 4 = radical violations)",
        dim: "SIT",
        kind: Kind::Ordinal,
        options: &["1", "2", "3", "4"],
    },
    // Human-elevated: temporal complexity
    Feature {
        key: "depth_recontextualization",
        name: "Depth of Recontextualization After Surprise",
        question: "How extensively does a revelation force reinterpretation of earlier scenes? (1 = none, 5 = complete re-reading)",
        dim: "REV",
        kind: Kind::Scale,
        options: &["1", "2", "3", "4", "5"],
    },
    Feature {
        key: "chronological_discontinuity",
        name: "Degree of Chronological Discontinuity",
        question: "How often does the narrative jump across time?",
        dim: "TMP",
        kind: Kind::Scale,
        options: &["1", "2", "3", "4", "5"],
    },
    Feature {
        key: "nonlinear_framing",
        name: "Nonlinear Framing for Delayed Disclosure",
        question: "To what extent does the story use time jumps to stage revelations? (1 = linear, 5 = heavily fragmented)",
        dim: "REV",
        kind: Kind::Scale,
        options: &["1", "2", "3", "4", "5"],
    },
    Feature {
        key: "anachrony_intensity",
        name: "Anachrony Intensity",
        question: "How heavily does the narrative rely on flashbacks or flash-forwards? (1 = absent, 5 = dominant anachronic)",
        dim: "TMP",
        kind: Kind::Scale,
        options: &["1", "2", "3", "4", "5"],
    },
    // Human-elevated: narrative diversity
    Feature {
        key: "location_variety",
        name: "Location Variety Scope",
        question: "How many distinct physical locales does the story inhabit?",
        dim: "SET",
        kind: Kind::Ordinal,
        options: &["single", "few_local", "regional", "global", "multiworld"],
    },
    Feature {
        key: "dialogue_to_narration",
        name: "Dialogue-to-Narration Proportion",
        question: "What proportion of text is direct dialogue vs. narration? (1 = no dialogue, 5 = dialogue dominates)",
        dim: "PER",
        kind: Kind::Scale,
        options: &["1", "2", "3", "4", "5"],
    },
    Feature {
        key: "moral_polarity",
        name: "Moral Polarity Toward Protagonist",
        question: "Does the narrative frame the protagonist's choices as morally clear or ambiguous?",
        dim: "PLT",
        kind: Kind::Cat,
        options: &["clearly_positive", "ambivalent_mixed", "clearly_negative"],
    },
];

pub fn find(key: &str) -> Option<&'static Feature> {
    FEATURES.iter().find(|f| f.key == key)
}

/// JSON schema for constrained decoding. Every feature is required, so a
/// truncated answer fails to parse instead of silently scoring as absent.
pub fn response_schema() -> Value {
    let mut props = serde_json::Map::new();
    for f in FEATURES {
        let schema = match f.kind {
            Kind::Scale | Kind::Ordinal => {
                json!({"type": "integer", "minimum": 1, "maximum": f.options.len()})
            }
            Kind::Cat => json!({"type": "string", "enum": f.options}),
            Kind::Multi => json!({
                "type": "array",
                "items": {"type": "string", "enum": f.options},
                "uniqueItems": true
            }),
        };
        props.insert(f.key.to_string(), schema);
    }
    json!({
        "type": "object",
        "properties": props,
        "required": FEATURES.iter().map(|f| f.key).collect::<Vec<_>>(),
        "additionalProperties": false
    })
}

/// The rubric handed to the rater. Ordinals are asked for as integers with
/// their labels spelled out, so a model that cannot follow an enum still lands
/// in range and stays comparable to the paper's integer coding.
pub fn rubric() -> String {
    let mut s = String::from(
        "You are annotating a work of narrative fiction for a literary study. \
Answer every question about the passage as a whole. Judge only what is on the page; \
do not speculate about what happens outside the excerpt. Answer with the JSON object \
described below and nothing else.\n\n",
    );
    for (i, f) in FEATURES.iter().enumerate() {
        s.push_str(&format!("{}. {} [{}]\n   {}\n   ", i + 1, f.name, f.key, f.question));
        match f.kind {
            Kind::Scale => s.push_str("Answer: integer 1-5.\n"),
            Kind::Ordinal => {
                let labels: Vec<String> = f
                    .options
                    .iter()
                    .enumerate()
                    .map(|(n, o)| format!("{}={}", n + 1, o))
                    .collect();
                s.push_str(&format!("Answer: integer, {}.\n", labels.join(", ")));
            }
            Kind::Cat => s.push_str(&format!("Answer: one of {}.\n", f.options.join(", "))),
            Kind::Multi => {
                s.push_str(&format!("Answer: array of any of {}.\n", f.options.join(", ")))
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_holds_thirty_distinct_features() {
        assert_eq!(FEATURES.len(), 30);
        let mut keys: Vec<_> = FEATURES.iter().map(|f| f.key).collect();
        keys.sort_unstable();
        let n = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), n, "duplicate feature key");
    }

    #[test]
    fn every_feature_offers_at_least_two_answers() {
        for f in FEATURES {
            assert!(f.options.len() >= 2, "{} has too few options", f.key);
        }
    }

    #[test]
    fn schema_requires_every_feature_and_bounds_numerics() {
        let s = response_schema();
        assert_eq!(s["required"].as_array().unwrap().len(), 30);
        assert_eq!(s["properties"]["thematic_explicitness"]["maximum"], 5);
        // Fourth-wall permeability is the one 1-4 ordinal in the set.
        assert_eq!(s["properties"]["fourth_wall_permeability"]["maximum"], 4);
        assert_eq!(s["additionalProperties"], false);
    }

    #[test]
    fn rubric_mentions_every_feature_key() {
        let r = rubric();
        for f in FEATURES {
            assert!(r.contains(f.key), "rubric omits {}", f.key);
        }
    }
}
