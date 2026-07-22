# Pilot: did a weaker annotator leave human/AI detection on the table?

StoryScope reads its features off each story with a single, older model. The
question that matters is not whether a stronger model annotates more
*accurately* — accuracy has no ground truth here — but whether stronger
annotation separates human from AI *better*. If it does, the weaker annotator
left detection signal unused.

## Method

- **Stories:** 22, all also in StoryScope's corpus so their published feature
  values are available for each. 12 AI (GPT-5.4, from their release) and 10
  public-domain human (6 Ambrose Bierce, 4 Bret Harte, from Gutenberg),
  median ~4,100 words.
- **Features:** the 40 most human/AI-discriminating of their 304, ranked by
  total-variation distance between human and AI in their *own* published data
  (`analysis/pilot_features.json`). Style and thematic features dominate.
- **Raters:** Claude Opus 4.8 and gpt-5.6-sol, single pass, annotating against
  StoryScope's exact taxonomy (IDs, wording, value sets) so values compare
  directly. No malformed answers.
- **Baseline:** StoryScope's own published values for the same 22 stories,
  treated as a third "rater" named `storyscope`.

## Results

**Concordance — strong raters mostly agree with the weak one, at the margin.**

| rater | exact match | within 1 |
| --- | --- | --- |
| Opus 4.8 | 62% | 98% |
| gpt-5.6-sol | 60% | 96% |

96-98% within one scale point means StoryScope's single-model annotation is not
grossly wrong; the disagreement is fine-grained.

**Separation — but the fine-grained disagreement carries detection signal.**
Mean human-vs-AI total-variation distance across the 40 features, on the
identical 22 stories:

| rater | mean TVD (higher separates better) |
| --- | --- |
| Opus 4.8 | 0.420 |
| gpt-5.6-sol | 0.335 |
| storyscope (published) | 0.284 |

Both independent frontier models, from different labs, separate human from AI
*more* than StoryScope's own published values do. They agree moderately on
which features gained (Spearman 0.48 on per-feature TVD). The asymmetry is
lopsided: **12 of 40 features where both strong raters beat storyscope by
>0.10; only 3 where storyscope beats both.** The gains concentrate in thematic
unity, sensory density, sentence complexity (parataxis/hypotaxis), literary
ambition, irony, and rhythm.

## What this does and does not show

**Does:** On identical stories, stronger annotation extracts features that
discriminate human from AI better than the weaker annotator's. Two independent
strong models corroborate. This is direct evidence that annotator strength is
a lever on detection, not just on annotation quality — the pilot's whole
question, answered yes.

**Does not:**
- **Absolute separation is era-inflated.** Our human sample is pre-1920
  public domain; 1890s prose vs 2026 AI separates more easily than modern
  human would. This inflates *all three* raters equally on the same stories,
  so it does not explain the strong > weak ranking (a clean within-story
  comparison) — but 0.42 is not a generalizable detection rate.
- **TVD is not classifier accuracy.** More per-feature separation is
  suggestive, not proof. The real test is whether a classifier trained on
  strong-model features generalizes to held-out *modern* human stories.
- **n is small** (10 human, 12 AI). The aggregate and the 12-vs-3 asymmetry
  are the trustworthy parts; per-feature numbers are noisy.

## Next

1. Rebuild the human arm with modern, DRM-free fiction (era-matched to the AI),
   so separation is not era-confounded.
2. Train the human-vs-AI classifier on strong-model features vs weak-model
   features; compare accuracy on a held-out set. TVD predicts the strong
   features should win; confirm it.
3. Bake-off downward: does a mid-tier or local model recover most of the
   separation gain at lower cost, before we scale to more books or all 304
   features?

Reproduce: `scripts/compare.py --annotations <our jsonl...> --reference
<storyscope published jsonl> --taxonomy <their taxonomy.json>`.
