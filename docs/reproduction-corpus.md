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

## What we do not need to buy

Chasing StoryScope's exact human books is the wrong plan: many are modern,
DRM-locked, and scattered across series (their 55 "Year's Best Science Fiction"
stories span ~35 annual Dozois volumes, ~$500). DRM matters because text
extraction requires parsing the file, and circumventing ebook DRM is itself
unlawful under US DMCA s.1201 regardless of the fair-use case for analysing the
text. We buy DRM-free or not at all.

We do not need exact-match on their modern books, because the questions that
need their specific stories can be answered on the free subset, and the
questions that need modern fiction do not need *their* fiction.

## Sequencing by cost

| Experiment | Data | Cost | DRM |
| --- | --- | --- | --- |
| 1. Rater reliability | their AI stories + their published features | free, in hand | none |
| 2. Pipeline exact-match validation | public-domain human stories (Gutenberg) + their AI stories, vs their features | free | none |
| 3. Era-controlled human/AI gap | modern human corpus + our own AI foil | cheap | DRM-free only |

1. **Rater reliability (free).** StoryScope released all AI story text and the
   304 published feature values for every AI story. Run our annotator on their
   AI stories, compare to their numbers: does a stronger/newer rater agree with
   their single-model pass, and where do they diverge? No human text, no
   purchase, no DRM.

2. **Pipeline validation (free).** ~500 of their human stories are by clearly
   public-domain authors (Twain 101, Bierce 81, Mansfield 73, Harris 66, Kipling
   52, O. Henry 39, Saki 36, Irving, Hawthorne, Chekhov, Chopin, Harte — verify
   each author's death date before relying on it; exclude anyone past ~1940).
   Free from Gutenberg/Standard Ebooks, DRM-free, and exact-matchable to their
   features by author+title. This validates our pipeline reproduces theirs
   before we extend it.

3. **Era-controlled gap (cheap, DRM-free).** For the modern arm, buy for
   availability, not exact-match: DRM-free SF/F ebook bundles (Humble Bundle,
   StoryBundle) or a few anthologies from DRM-free sellers (Baen, Tor/Macmillan,
   Weightless Books). We generate the AI foil ourselves. Label era across both
   arms and test which features still separate human from AI in the modern arm
   alone.

## Legal posture

Purchase grants a licence to read; computational analysis that publishes only
derived numeric features (never the text) is the Google Books Ngram /
HathiTrust fact pattern and is far stronger than Books3's. We publish feature
values, never prose, and never strip DRM. See the corpus survey in the private
notes for the full source-by-source licence review.
