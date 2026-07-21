# Reproducing StoryScope's human corpus with clean provenance

StoryScope sourced its 10,272 human stories from Books3 (pirated) and, rightly,
did not redistribute the text. It *did* release the source metadata —
`human_author`, `human_anthology`, `human_word_count`, `title` — for
reproducibility. That metadata is a shopping list: buy the same books legally,
re-extract the same stories, and compare our feature annotations against their
published values on identical text.

`docs/storyscope_sources.csv` is that list, derived from their released
`stories_{split}.parquet` metadata columns (no story text was downloaded — only
the metadata columns, via HTTP column projection).

## What the corpus actually is

| | |
| --- | --- |
| Stories | 10,272 |
| Source books ("anthologies") | 699 |
| Distinct authors | 3,048 |
| Median length | 4,910 words (mean 6,242) |
| Length: 2–8k words | 79% |

Their `human_anthology` field is the **source book**, not necessarily a
multi-author anthology:

- **234 multi-author anthologies** → 3,995 stories. Buyable as one unit,
  story-dense (one *Year's Best Science Fiction* = 55 matched stories),
  and predominantly **modern and in-copyright**.
- **465 single/dual-author collections** → 6,277 stories. Dominated by
  **public-domain classics** (Twain 101 stories, L'Amour 91, Hughes 82, Bierce
  81, Mansfield 73, Saki, O. Henry, Uncle Remus).

## The era confound, confirmed inside their own data

The human set silently spans the 1890s to the 2020s; the AI set is 2026. A
human-vs-AI classifier trained on it is therefore **partly a period
classifier** — it can win by detecting 1890s prose vs 2020s prose rather than
human vs AI. StoryScope does not control for this.

This is testable, and it is the first expansion this project should make. Buy
both arms, label era, and ask of every feature: does it separate human from AI
in the **modern** arm alone? Features that only work once pre-1930 prose is
mixed in are detecting period, not authorship.

## Acquisition plan

1. **Modern arm — buy.** Work down the multi-author anthologies in
   `storyscope_sources.csv` (type = `multi-author`). A dozen covers ~400
   matched stories, two dozen ~700. Top of the list: the *Year's Best SF*,
   *Best SF & Fantasy of the Year*, *Best New Horror*, and *Year's Best Dark
   Fantasy* volumes, plus *Africa39*, *Echoes*, *Blackguards*, *Thriller*. All
   in print as ebooks.
2. **Classic arm — free.** The public-domain single-author collections
   (Twain, Bierce, O. Henry, Saki, Mansfield, Hughes, Harris) come from Project
   Gutenberg and Standard Ebooks with clean chain of title. No purchase.
3. **Extract and join.** Re-extract bought/downloaded stories, join on
   `title` / `prompt_id` to StoryScope's published `storyscope_features.parquet`
   and to their five AI-generated counterparts. This gives a matched
   comparison: same story, our (newer, multi-rater) annotation vs their
   single-model annotation, against a shared AI foil.

## Legal posture

Purchase grants a licence to read; computational analysis that publishes only
derived numeric features (never the text) is the Google Books Ngram /
HathiTrust fact pattern and is far stronger than Books3's. We publish feature
values, never prose. See the corpus survey in the private notes for the full
source-by-source licence review.
