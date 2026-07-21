# turing-tells

Measuring what separates human from AI narrative fiction — and, first,
measuring whether those measurements can be trusted.

## Why

Recent work (notably StoryScope, [Russell et al.][ss]) reports that a
classifier reading narrative-craft features off a story — how explicitly it
moralizes, how it conveys emotion, how it handles time — separates human from
AI fiction at ~93%. The features are read off each story by a single language
model in a single pass.

That last detail is the one this project starts with. If the feature values are
produced by one model with no inter-rater check, they may be properties of the
annotator rather than of the text. Whether they are is measurable, and it
should be measured before anything is built on top of the numbers.

## `narrative-features`

Asks StoryScope's 30 core features — verbatim, since the question wording *is*
the measuring instrument — of a passage, and scores how reliably a model
answers them.

Two backends: a local OpenAI-compatible server with JSON-schema-constrained
decoding (malformed answers become impossible), or an arbitrary shell command
for frontier CLIs, where the schema can only be requested and is repaired from
chatty replies.

Reliability is agreement, because these annotations have no ground truth. The
headline statistic is **Krippendorff's alpha**, which corrects for the
agreement chance would produce given the observed answer distribution; raw
percent agreement, which flatters features whose answer is nearly always the
same, is reported alongside it. Alpha spans the three answer shapes through its
difference function — squared distance for scales and ordinals, 0/1 for
categoricals, Jaccard for multi-selects. A feature answered identically across
the whole corpus is reported as *undefined*, not perfect: it is measuring
nothing, whatever its agreement looks like.

```
# 1. Assemble story-scale passages from a directory of text chunks
narrative-features sample --chunks ./chunks --out ./passages --join 3

# 2. Ask a rater the 30 questions (repeat --endpoint to spread across machines,
#    or use --command "claude -p" for a frontier CLI). Resumable.
narrative-features annotate --passages ./passages --rater gemma-4-31b \
    --endpoint http://127.0.0.1:8080 --runs 3 --out data/features.jsonl

# 3. Agreement per feature.
#    --within R: one rater's self-consistency across its runs
#    --aggregate: pool each rater's runs, then compare raters
narrative-features report --input data/features.jsonl --aggregate
```

Results append to JSONL keyed by (passage, rater, run), so an interrupted sweep
resumes without re-spending tokens on work already done.

## Early finding

On 12 human-written passages, three raters (Gemma 4 31B local, Claude Opus 4.8,
gpt-5.6-sol): frontier models are stable with themselves (alpha .76) and
disagree with each other (.60). Each holds a consistent idiosyncratic reading
of the rubric, so absolute feature values are annotator-dependent. Pooling
repeated runs lifts every rater by a similar margin and does not change the
ranking. This does not by itself void a human-vs-AI *gap*, where a constant
rater bias cancels; it voids absolute thresholds and any scheme that steers
prose toward fixed feature values.

Caveats bounding that: n=12, all human-written, one broad genre, and the
passages are mid-novel excerpts rather than complete stories — so features
about how a story *resolves* are ill-posed. A fuller corpus of complete short
stories, human and AI at matched length, is the next step.

## Status

Early and honest. The instrument is being characterized before the result is
reproduced. Findings here supersede nothing until the corpus is sound.

## License

Code is MIT (`LICENSE`). StoryScope's feature definitions are used under the
terms of their release; this is an independent reproduction and extension, not
affiliated with its authors.

[ss]: https://github.com/jenna-russell/storyscope
